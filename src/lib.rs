pub mod nbp_parser;

use askama::Template;
use axum::Json;
use axum::error_handling::HandleErrorLayer;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::{Router, routing::get};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower::BoxError;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info};

use nbp_parser::{CurrencyExchangeRateItem, ParseError, parse_nbp_xml};

#[derive(Template)]
#[template(path = "index.html", escape = "html")]
struct IndexTemplate {
    exchange_rate_items: Vec<CurrencyExchangeRateItem>,
}

struct CachedRates {
    fetched_at: Instant,
    items: Vec<CurrencyExchangeRateItem>,
}

fn cache_ttl() -> Duration {
    std::env::var("NBP_CACHE_TTL")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(3600))
}

#[derive(Clone)]
pub struct AppState {
    nbp_url: String,
    client: reqwest::Client,
    cache: Arc<RwLock<Option<CachedRates>>>,
    cache_ttl: Duration,
}

#[derive(Debug, thiserror::Error)]
enum UpstreamError {
    #[error("Upstream request timed out")]
    Timeout,

    #[error("Upstream returned {0}")]
    BadStatus(reqwest::StatusCode),

    #[error("Failed to parse upstream response: {0}")]
    Parse(#[from] ParseError),
}

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("Upstream error: {0}")]
    Upstream(#[from] UpstreamError),

    #[error("Network error: {0}")]
    Network(reqwest::Error),

    #[error("Failed to render template: {0}")]
    Template(#[from] askama::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Upstream(_) => {
                tracing::error!("{self}");
                (StatusCode::BAD_GATEWAY, self.to_string()).into_response()
            }
            AppError::Network(_) | AppError::Template(_) => {
                error!("{self}");
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
            }
        }
    }
}

fn classify_reqwest_error(err: reqwest::Error) -> AppError {
    if err.is_timeout() {
        UpstreamError::Timeout.into()
    } else if let Some(status) = err.status() {
        UpstreamError::BadStatus(status).into()
    } else {
        AppError::Network(err)
    }
}

pub fn create_router(nbp_url: String) -> Router {
    Router::new()
        .route("/", get(handler))
        .with_state(AppState {
            nbp_url,
            client: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(None)),
            cache_ttl: cache_ttl(),
        })
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_error))
                .timeout(Duration::from_secs(30))
                .layer(TraceLayer::new_for_http()),
        )
}

async fn get_exchange_rates(state: &AppState) -> Result<Vec<CurrencyExchangeRateItem>, AppError> {
    if let Some(ref c) = *state.cache.read().await
        && c.fetched_at.elapsed() < state.cache_ttl
    {
        debug!("Serving from cache");
        return Ok(c.items.clone());
    }

    let mut cache = state.cache.write().await;
    // double-check locking, in case someone beats us to it
    if let Some(ref c) = *cache
        && c.fetched_at.elapsed() < state.cache_ttl
    {
        debug!("Serving from cache on a write lock");
        return Ok(c.items.clone());
    }

    info!("Performing a request to NBP");
    let items = fetch_from_upstream(state).await?;
    *cache = Some(CachedRates {
        fetched_at: Instant::now(),
        items: items.clone(),
    });
    Ok(items)
}

async fn fetch_from_upstream(state: &AppState) -> Result<Vec<CurrencyExchangeRateItem>, AppError> {
    let response = state
        .client
        .get(&state.nbp_url)
        .send()
        .await
        .map_err(classify_reqwest_error)?
        .error_for_status()
        .map_err(classify_reqwest_error)?;
    let body = response.text().await.map_err(classify_reqwest_error)?;
    parse_nbp_xml(&body)
        .map_err(UpstreamError::from)
        .map_err(AppError::from)
}

async fn handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let exchange_rates = get_exchange_rates(&state).await?;

    // TODO: proper content negotiation, in case of `text/html;q=0.9, application/json`
    let accepts_html = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/html"));

    if accepts_html {
        let html = IndexTemplate {
            exchange_rate_items: exchange_rates,
        }
        .render()?;
        Ok(Html(html).into_response())
    } else {
        Ok(Json(exchange_rates).into_response())
    }
}

async fn handle_error(err: BoxError) -> (StatusCode, String) {
    if err.is::<tower::timeout::error::Elapsed>() {
        (
            StatusCode::REQUEST_TIMEOUT,
            "Request took too long".to_string(),
        )
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Unhandled internal error: {err}"),
        )
    }
}
