// 2022-2024 (c) Copyright Contributors to the GOSH DAO. All rights reserved.
//

use std::net::SocketAddr;
use std::time::Duration;
use std::time::SystemTime;

use chitchat::spawn_chitchat;
use chitchat::ChitchatConfig;
use chitchat::ChitchatHandle;
use chitchat::ChitchatId;
use chitchat::ChitchatRef;
use chitchat::ClusterStateSnapshot;
use chitchat::FailureDetectorConfig;
use cool_id_generator::Size;
use poem::listener::TcpListener;
use poem::middleware::Cors;
use poem::EndpointExt;
use poem::Route;
use poem::Server;
use poem_openapi::OpenApiService;
use serde::Deserialize;
use serde::Serialize;
use tokio::task::JoinHandle;

static DEFAULT_GOSSIP_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse {
    pub cluster_id: String,
    pub cluster_state: ClusterStateSnapshot,
    pub live_nodes: Vec<ChitchatId>,
    pub dead_nodes: Vec<ChitchatId>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetKeyValueResponse {
    pub status: bool,
}

fn generate_server_id(public_addr: SocketAddr) -> String {
    let cool_id = cool_id_generator::get_id(Size::Medium);
    format!("server:{public_addr}-{cool_id}")
}

pub async fn run(
    listen_addr: SocketAddr,
    api_addr: SocketAddr,
    transport: impl chitchat::transport::Transport,
    gossip_advertise_addr: SocketAddr,
    seeds: Vec<String>,
    cluster_id: String,
    initial_key_values: Vec<(String, String)>,
) -> anyhow::Result<(ChitchatRef, ChitchatHandle, JoinHandle<anyhow::Result<()>>)> {
    let node_id = generate_server_id(gossip_advertise_addr);
    let generation = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let chitchat_id = ChitchatId::new(node_id, generation, gossip_advertise_addr);
    let config = ChitchatConfig {
        cluster_id,
        chitchat_id,
        gossip_interval: DEFAULT_GOSSIP_INTERVAL,
        listen_addr,
        seed_nodes: seeds.clone(),
        failure_detector_config: FailureDetectorConfig::default(),
        marked_for_deletion_grace_period: Duration::from_secs(10), // TODO: extract hardcoded value
        catchup_callback: None,
        extra_liveness_predicate: None,
    };

    tracing::info!("Starting gossip server on {gossip_advertise_addr}");
    let chitchat_handle = spawn_chitchat(config, initial_key_values, &transport).await?;
    let chitchat = chitchat_handle.chitchat();
    let api = crate::open_api::routes::Api { chitchat: chitchat.clone() };

    let version = env!("CARGO_PKG_VERSION");
    let description = env!("CARGO_PKG_DESCRIPTION");

    let api_service = OpenApiService::new(api, description, version);
    let docs = api_service.swagger_ui();

    let app = Route::new() //
        .nest("/", api_service)
        .nest("/docs", docs)
        .with(Cors::new());

    tracing::info!("Starting REST API server on listen addr {api_addr}");

    let rest_server_handle = tokio::spawn(async move {
        Server::new(TcpListener::bind(api_addr)).run(app).await.map_err(|err| err.into())
    });

    Ok((chitchat, chitchat_handle, rest_server_handle))
}
