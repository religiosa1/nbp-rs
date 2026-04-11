use std::env::var;
use tracing::info;

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

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("shutdown signal received");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nbp_rs=debug,tower_http=debug,info".into()),
        )
        .init();

    let nbp_url = var("NBP_URL").unwrap_or("https://rss.nbp.pl/kursy/TabelaA.xml".into());
    let app = nbp_rs::create_router(nbp_url);

    match bind_listener().await {
        Listener::Tcp(l) => {
            info!("listening on {}", l.local_addr().unwrap());
            axum::serve(l, app)
                .with_graceful_shutdown(shutdown_signal())
                .await
                .unwrap();
        }
        Listener::Unix(l) => {
            info!("listening on unix socket");
            axum::serve(l, app)
                .with_graceful_shutdown(shutdown_signal())
                .await
                .unwrap();
        }
    }

    info!("server stopped");
}
