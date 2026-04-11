use std::net::SocketAddr;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const VALID_NBP_XML: &str = r#"<rss xmlns:atom="http://www.w3.org/2005/Atom" version="2.0">
  <channel>
    <item>
      <title>Tabela nr 069/A/NBP/2026 z dnia 2026-04-10</title>
      <description><![CDATA[ <table>
        <tr align="right"><td>1 EUR =</td><td>4,2534</td></tr>
        <tr align="right"><td>1 USD =</td><td>3,6396</td></tr>
        <tr align="right"><td>1 CHF =</td><td>4,6062</td></tr>
        <tr align="right"><td>1 GBP =</td><td>4,8848</td></tr>
        <tr align="right"><td>100 JPY =</td><td>2,2845</td></tr>
      </table> ]]></description>
      <link>http://rss.nbp.pl/kursy/TabRss.aspx?n=2026/a/26a069</link>
      <pubDate>Fri, 10 Apr 2026 11:45:02 +0200</pubDate>
    </item>
  </channel>
</rss>"#;

async fn spawn_app(nbp_url: String) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let app = nbp_rs::create_router(nbp_url);
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

#[tokio::test]
async fn happy_path_json() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(VALID_NBP_XML))
        .mount(&mock_server)
        .await;

    let addr = spawn_app(mock_server.uri()).await;
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/"))
        .header("Accept", "application/json")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert!(resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("application/json"));

    let body: serde_json::Value = resp.json().await.unwrap();
    let items = body.as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Tabela nr 069/A/NBP/2026 z dnia 2026-04-10");
    assert_eq!(items[0]["rates"]["eur"], 4.2534);
    assert_eq!(items[0]["rates"]["usd"], 3.6396);
}

#[tokio::test]
async fn happy_path_html() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(VALID_NBP_XML))
        .mount(&mock_server)
        .await;

    let addr = spawn_app(mock_server.uri()).await;
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/"))
        .header("Accept", "text/html")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert!(resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("text/html"));

    let body = resp.text().await.unwrap();
    assert!(body.contains("Tabela nr 069/A/NBP/2026 z dnia 2026-04-10"));
    assert!(body.contains("<details"));
}

#[tokio::test]
async fn upstream_bad_status_returns_502() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&mock_server)
        .await;

    let addr = spawn_app(mock_server.uri()).await;
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 502);
}

#[tokio::test]
async fn upstream_malformed_xml_returns_502() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not xml at all"))
        .mount(&mock_server)
        .await;

    let addr = spawn_app(mock_server.uri()).await;
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 502);
}
