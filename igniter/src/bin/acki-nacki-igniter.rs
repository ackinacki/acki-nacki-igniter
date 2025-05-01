use core::panic;
use std::process::exit;
use std::thread;

use acki_nacki_igniter::cli::CLI;
use acki_nacki_igniter::gossip;
use acki_nacki_igniter::open_api::server;
use acki_nacki_igniter::transport;
use acki_nacki_igniter::IGNITER_IMAGE;
use ed25519_dalek::SigningKey;
use tracing::error;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use updater::ContainerUpdater;
use updater::DEFAULT_UPDATE_INTERVAL;

fn main() {
    _ = *CLI; // make sure we have the value or panic before we start

    eprintln!("Starting server: advertise address {}", CLI.config.advertise_addr);
    eprintln!("Settings: {:#?}", &*CLI);

    thread::Builder::new()
        .name("tokio_main".to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(tokio_main)
        .expect("failed to spawn tokio thread")
        .join()
        .expect("tokio thread panicked");

    unreachable!();
}

#[tokio::main]
pub async fn tokio_main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer() //
                .with_file(true)
                .with_line_number(true),
        )
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    if let Err(err) = tokio_main_inner().await {
        error!(error=?err, "tokio_main failed");
        exit(1);
    }
    exit(0);
}

async fn tokio_main_inner() -> anyhow::Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    let commit = env!("BUILD_GIT_COMMIT");
    info!("Starting server: version {} (commit {})", current_version, commit);

    let updater = ContainerUpdater::try_new(
        IGNITER_IMAGE.to_owned(),
        DEFAULT_UPDATE_INTERVAL,
        CLI.docker_socket.clone(),
    )
    .await?;

    let params = CLI.clone();

    let updater_handle = tokio::spawn(async move {
        if params.config.auto_update {
            info!("Auto update enabled");
            updater.run().await
        } else {
            info!("Auto update disabled");
            std::future::pending().await
        }
    });

    let initial_key_values = params.to_gossip()?;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());

    let udp_transport = transport::signed_udp::UdpSignedTransport::new(
        vec![],
        signing_key,
        chitchat::transport::UdpTransport,
    );

    let gossip_state = gossip::run(initial_key_values, &udp_transport).await?;

    let rest_server_handle = tokio::spawn(server::run(gossip_state.chitchat()));

    tokio::select! {
        v = updater_handle => {
            anyhow::bail!("Container updater failed: {v:?}");
        }

        v = gossip_state.join_handle => {
            anyhow::bail!("Gossip server failed: {v:?}");
        }
        v = rest_server_handle => {
            anyhow::bail!("API server failed: {v:?}");
        }
    }
}
