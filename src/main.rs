use std::env::{args, var};
use std::io;
use tracing::info;

enum Listener {
    Tcp(tokio::net::TcpListener),
    Unix(tokio::net::UnixListener),
}

impl Listener {
    pub async fn bind() -> io::Result<Self> {
        let mut listenfd = listenfd::ListenFd::from_env();
        if let Some(listener) = listenfd.take_unix_listener(0).map_err(|e| {
            io::Error::new(e.kind(), format!("failed to acquire systemd socket: {e}"))
        })? {
            listener.set_nonblocking(true).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("failed to set systemd socket non-blocking: {e}"),
                )
            })?;
            return Ok(Self::Unix(
                tokio::net::UnixListener::from_std(listener).map_err(|e| {
                    io::Error::new(
                        e.kind(),
                        format!("failed to create async listener from systemd socket: {e}"),
                    )
                })?,
            ));
        }

        let addr = var("NBP_ADDR").unwrap_or("127.0.0.1:3000".into());

        match addr.starts_with('/') {
            true => tokio::net::UnixListener::bind(&addr)
                .map(Self::Unix)
                .map_err(|e| {
                    io::Error::new(
                        e.kind(),
                        format!("failed to bind unix socket {addr:?}: {e}"),
                    )
                }),
            false => tokio::net::TcpListener::bind(&addr)
                .await
                .map(Self::Tcp)
                .map_err(|e| {
                    io::Error::new(
                        e.kind(),
                        format!("failed to bind TCP address {addr:?}: {e}"),
                    )
                }),
        }
    }

    pub fn local_addr(&self) -> io::Result<String> {
        match self {
            Self::Tcp(l) => Ok(l.local_addr()?.to_string()),
            Self::Unix(l) => Ok(l
                .local_addr()?
                .as_pathname()
                .and_then(|p| p.to_str())
                .unwrap_or("unix socket")
                .to_string()),
        }
    }
}

#[tokio::main]
async fn main() {
    if args().any(|a| a == "--version" || a == "-V") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if args().any(|a| a == "--help" || a == "-h") {
        println!(
            "{name} {version}
{description}

USAGE:
    {name} [OPTIONS]

OPTIONS:
    -h, --help       Print this help message
    -V, --version    Print version

ENVIRONMENT:
    NBP_URL          URL of the NBP RSS feed to fetch
                     [default: https://rss.nbp.pl/kursy/TabelaA.xml]
    NBP_ADDR         TCP address or unix socket path to bind to
                     [default: 127.0.0.1:3000]
                     Ignored when a socket is passed via systemd socket activation (FD#3)
    NBP_CACHE_TTL    How long to cache upstream responses, in seconds
                     [default: 3600]
    RUST_LOG         Log verbosity filter
                     Examples: warn | info | debug | trace
                               nbp_rs=debug,tower_http=debug,info
                     [default: nbp_rs=debug,tower_http=debug,info]",
            name = env!("CARGO_PKG_NAME"),
            version = env!("CARGO_PKG_VERSION"),
            description = env!("CARGO_PKG_DESCRIPTION"),
        );
        return;
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nbp_rs=debug,tower_http=debug,info".into()),
        )
        .init();

    let nbp_url = var("NBP_URL").unwrap_or("https://rss.nbp.pl/kursy/TabelaA.xml".into());
    let cache_ttl = std::time::Duration::from_secs(
        var("NBP_CACHE_TTL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3600),
    );
    let app = nbp_rs::create_router(nbp_url, cache_ttl);

    let listener = Listener::bind().await.unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });
    info!(
        "listening on {}",
        listener.local_addr().unwrap_or_else(|_| "unknown".into())
    );

    match listener {
        Listener::Tcp(l) => axum::serve(l, app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap(),
        Listener::Unix(l) => axum::serve(l, app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap(),
    }

    info!("server stopped");
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
