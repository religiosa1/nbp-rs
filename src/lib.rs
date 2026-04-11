pub mod nbp_parser;

use askama::Template;
use axum::Json;
use axum::error_handling::HandleErrorLayer;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::{Router, routing::get};
use std::time::Duration;
use tower::BoxError;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::error;

use nbp_parser::{CurrencyExchangeRateItem, ParseError, parse_nbp_xml};

#[derive(Template)]
#[template(path = "index.html", escape = "html")]
struct IndexTemplate {
    exchange_rate_items: Vec<CurrencyExchangeRateItem>,
}

#[derive(Clone)]
pub struct AppState {
    pub nbp_url: String,
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
                error!("{self}");
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
        .with_state(AppState { nbp_url })
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_error))
                .timeout(Duration::from_secs(30))
                .layer(TraceLayer::new_for_http()),
        )
}

async fn get_exchange_rates(url: &str) -> Result<Vec<CurrencyExchangeRateItem>, AppError> {
    let response = reqwest::get(url)
        .await
        .map_err(classify_reqwest_error)?
        .error_for_status()
        .map_err(classify_reqwest_error)?;
    let body = response.text().await.map_err(classify_reqwest_error)?;
    Ok(parse_nbp_xml(&body).map_err(UpstreamError::from)?)
}

async fn handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let exchange_rates = get_exchange_rates(&state.nbp_url).await?;

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
