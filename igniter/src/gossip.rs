use std::net::SocketAddr;
use std::time::Duration;
use std::time::SystemTime;

use chitchat::spawn_chitchat;
use chitchat::transport::Transport;
use chitchat::ChitchatConfig;
use chitchat::ChitchatHandle;
use chitchat::ChitchatId;
use chitchat::FailureDetectorConfig;
use cool_id_generator::Size;
use tracing::debug;
use tracing::info;

use crate::cli::CLI;

fn generate_server_id(public_addr: SocketAddr) -> String {
    let cool_id = cool_id_generator::get_id(Size::Medium);
    format!("server:{public_addr}-{cool_id}")
}

pub async fn run(
    initial_key_values: Vec<(String, String)>,
    transport: &dyn Transport,
) -> anyhow::Result<ChitchatHandle> {
    let version = env!("CARGO_PKG_VERSION");
    let name = env!("CARGO_PKG_NAME");
    let description = env!("CARGO_PKG_DESCRIPTION");
    info!("Starting gossip {name} {version} {description}");

    let generation_id = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Unix time before EPOCH")
        .as_secs();
    debug!(?generation_id, "generation id");

    let chitchat_id = ChitchatId::new(
        CLI.config.node_id.clone().unwrap_or_else(|| generate_server_id(CLI.config.advertise_addr)),
        generation_id,
        CLI.config.advertise_addr,
    );
    debug!(?chitchat_id, "chitchat id");

    let cluster_id = name.to_string();
    debug!(?cluster_id, "cluster id");

    let config = ChitchatConfig {
        chitchat_id,
        cluster_id,
        gossip_interval: Duration::from_millis(CLI.config.interval),
        listen_addr: CLI.config.listen_addr,
        seed_nodes: CLI.config.seeds.clone(),
        failure_detector_config: FailureDetectorConfig {
            dead_node_grace_period: Duration::from_secs(10),
            ..FailureDetectorConfig::default()
        },
        marked_for_deletion_grace_period: Duration::from_secs(60),
        catchup_callback: None,
        extra_liveness_predicate: None,
    };

    info!("spawn chitchat");
    let chitchat_handler = spawn_chitchat(config, initial_key_values, transport).await?;

    Ok(chitchat_handler)
}
