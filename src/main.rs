use std::env::var;

enum Listener {
    Tcp(tokio::net::TcpListener),
    Unix(tokio::net::UnixListener),
}

async fn bind_listener() -> Listener {
    let mut listenfd = listenfd::ListenFd::from_env();
    if let Some(listener) = listenfd.take_unix_listener(0).unwrap() {
        listener.set_nonblocking(true).unwrap();
        return Listener::Unix(tokio::net::UnixListener::from_std(listener).unwrap());
    }

    if let Ok(addr) = var("NBP_ADDR") {
        if addr.starts_with('/') {
            return Listener::Unix(tokio::net::UnixListener::bind(&addr).unwrap());
        } else {
            return Listener::Tcp(tokio::net::TcpListener::bind(&addr).await.unwrap());
        }
    }

    Listener::Tcp(tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap())
}

#[tokio::main]
async fn main() {
    let nbp_url = var("NBP_URL").unwrap_or("https://rss.nbp.pl/kursy/TabelaA.xml".into());
    let app = nbp_rs::create_router(nbp_url);

    match bind_listener().await {
        Listener::Tcp(l) => {
            println!("listening on {}", l.local_addr().unwrap());
            axum::serve(l, app).await.unwrap();
        }
        Listener::Unix(l) => {
            println!("listening on unix socket");
            axum::serve(l, app).await.unwrap();
        }
    }
}
