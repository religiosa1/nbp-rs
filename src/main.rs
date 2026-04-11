use axum::serve::Listener as AxumListener;
use std::env::{args, var};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::info;

trait Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> Stream for T {}
type DynStream = Box<dyn Stream>;

enum Listener {
    Tcp(tokio::net::TcpListener),
    Unix(tokio::net::UnixListener),
}

impl Listener {
    pub async fn bind() -> Self {
        let mut listenfd = listenfd::ListenFd::from_env();
        if let Some(listener) = listenfd.take_unix_listener(0).unwrap() {
            listener.set_nonblocking(true).unwrap();
            return Self::Unix(tokio::net::UnixListener::from_std(listener).unwrap());
        }

        if let Ok(addr) = var("NBP_ADDR") {
            if addr.starts_with('/') {
                return Self::Unix(tokio::net::UnixListener::bind(&addr).unwrap());
            } else {
                return Self::Tcp(tokio::net::TcpListener::bind(&addr).await.unwrap());
            }
        }

        Self::Tcp(
            tokio::net::TcpListener::bind("127.0.0.1:3000")
                .await
                .unwrap(),
        )
    }
}

impl axum::serve::Listener for Listener {
    type Io = DynStream;
    type Addr = String;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self {
            Self::Tcp(l) => {
                let (stream, addr) = tokio::net::TcpListener::accept(l).await.unwrap();
                (Box::new(stream) as DynStream, addr.to_string())
            }
            Self::Unix(l) => {
                let (stream, addr) = tokio::net::UnixListener::accept(l).await.unwrap();
                let addr_str = addr
                    .as_pathname()
                    .and_then(|p: &std::path::Path| p.to_str())
                    .unwrap_or("unix socket") // FIXME: log and continue instead
                    .to_string();
                (Box::new(stream) as DynStream, addr_str)
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(match self {
            Self::Tcp(l) => l.local_addr()?.to_string(),
            Self::Unix(l) => l
                .local_addr()?
                .as_pathname()
                .and_then(|p| p.to_str())
                .unwrap_or("unix socket")
                .to_string(),
        })
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
    let app = nbp_rs::create_router(nbp_url);

    let listener = Listener::bind().await;
    info!(
        "listening on {}",
        AxumListener::local_addr(&listener).unwrap()
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

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
