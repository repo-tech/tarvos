use std::{env, net::SocketAddr};

use axum::Router;
use tarvos_core::api::gateway::{router, GatewayConfig};

const DEFAULT_PORT: u16 = 8080;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port = match env::var("PORT")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        Some(value) => match value.parse::<u16>() {
            Ok(port) => port,
            Err(error) => {
                eprintln!("invalid PORT={value:?}: {error}; using {DEFAULT_PORT}");
                DEFAULT_PORT
            }
        },
        None => DEFAULT_PORT,
    };
    let bind_address = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_owned());
    let address: SocketAddr = format!("{bind_address}:{port}").parse()?;
    let app: Router = router(GatewayConfig::default());
    let listener = tokio::net::TcpListener::bind(address).await?;

    println!(
        "Tarvos v1.1.0-rc.3 gateway listening on http://{}",
        listener.local_addr()?
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            eprintln!("failed to install Ctrl+C handler: {error}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => eprintln!("failed to install SIGTERM handler: {error}"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
