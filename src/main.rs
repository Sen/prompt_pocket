use std::net::SocketAddr;

use prompt_pocket::{create_router, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let config = prompt_pocket::config::Config::from_env();
    let port = config.port;
    let state = AppState::new(config).await?;
    let router = create_router(state);
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(address).await?;

    println!("Prompt Pocket is running at http://{address}");
    axum::serve(listener, router).await?;

    Ok(())
}
