use std::env::var;

#[tokio::main]
async fn main() {
    let nbp_url = var("NBP_URL").unwrap_or("https://rss.nbp.pl/kursy/TabelaA.xml".into());
    let app = nbp_rs::create_router(nbp_url);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
