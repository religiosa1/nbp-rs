use askama::Template;
use axum::Json;
use axum::error_handling::HandleErrorLayer;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::{Router, routing::get};
use std::env::var;
use std::time::Duration;
use tower::BoxError;
use tower::ServiceBuilder;

use crate::nbp_parser::{CurrencyExchangeRateItem, parse_nbp_xml};
mod nbp_parser;

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    exchange_rate_items: Vec<CurrencyExchangeRateItem>,
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(handler)).layer(
        ServiceBuilder::new()
            .layer(HandleErrorLayer::new(handle_error))
            .timeout(Duration::from_secs(30)),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn get_exchange_rates() -> Result<Vec<CurrencyExchangeRateItem>, anyhow::Error> {
    let url = var("NBP_URL").unwrap_or("https://rss.nbp.pl/kursy/TabelaA.xml".into());
    let body = reqwest::get(url).await?.text().await?;
    parse_nbp_xml(&body)
}

async fn handler(headers: HeaderMap) -> Result<impl IntoResponse, AppError> {
    let exchange_rates = get_exchange_rates().await?;

    let accepts_html = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/html"));

    if accepts_html {
        let html = IndexTemplate {
            exchange_rate_items: exchange_rates,
        }
        .render()
        .map_err(anyhow::Error::from)?;
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

struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(err: E) -> Self {
        AppError(err.into())
    }
}
