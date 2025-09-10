use core::panic;
use std::process::exit;
use std::thread;

use acki_nacki_igniter::cli::CLI;
use acki_nacki_igniter::IGNITER_IMAGE;
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

    let params = CLI.clone();

    let updater_handle = tokio::spawn(async move {
        if params.config.auto_update {
            info!("Auto update enabled");
            let updater = ContainerUpdater::try_new(
                IGNITER_IMAGE.to_owned(),
                DEFAULT_UPDATE_INTERVAL,
                CLI.docker_socket.clone(),
                CLI.docker_config.clone(),
            )
            .await?;

            updater.run().await
        } else {
            info!("Auto update disabled");
            std::future::pending().await
        }
    });

    let initial_key_values = params.to_gossip()?;

    let listen_addr = CLI.config.listen_addr;
    let api_addr = CLI.config.api_addr;
    let seeds = CLI.config.seeds.clone();
    let advertise_addr = CLI.config.advertise_addr;
    let cluster_id = CLI.config.cluster_id.clone();

    tracing::info!("Gossip advertise addr: {:?}", advertise_addr);

    let (chitchat, gossip_handle, gossip_rest_handle) = acki_nacki_igniter::gossip::run(
        listen_addr,
        api_addr,
        chitchat::transport::UdpTransport,
        advertise_addr,
        seeds,
        cluster_id,
        initial_key_values,
    )
    .await?;

    let revoked_licenses_watcher =
        acki_nacki_igniter::revoked_license_watcher::run(chitchat, params.keys.wallet.pubkey).await;

    tokio::select! {
        v = updater_handle => {
            anyhow::bail!("Container updater failed: {v:?}");
        }

        v = gossip_handle.join_handle => {
            anyhow::bail!("Gossip server failed: {v:?}");
        }
        v = gossip_rest_handle => {
            anyhow::bail!("API server failed: {v:?}");
        }
         v = revoked_licenses_watcher => {
            anyhow::bail!("License watcher failed: {v:?}");
        }
    }
}
