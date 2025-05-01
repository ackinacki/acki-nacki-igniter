use std::sync::Arc;

use chitchat::Chitchat;
use poem::listener::TcpListener;
use poem::middleware::Cors;
use poem::EndpointExt;
use poem::Route;
use poem::Server;
use poem_openapi::OpenApiService;
use tokio::sync::Mutex;

use crate::cli::CLI;
use crate::open_api::routes::Api;

pub async fn run(gossip: Arc<Mutex<Chitchat>>) -> anyhow::Result<()> {
    let api = Api::new(gossip);

    let version = env!("CARGO_PKG_VERSION");
    let description = env!("CARGO_PKG_DESCRIPTION");

    let api_service = OpenApiService::new(api, description, version);
    let docs = api_service.swagger_ui();

    Server::new(TcpListener::bind(&CLI.config.api_addr))
        .run(
            Route::new() //
                .nest("/", api_service)
                .nest("/docs", docs)
                .with(Cors::new()),
        )
        .await?;
    Ok(())
}
