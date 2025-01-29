use core::panic;
use std::path::Path;
use std::path::PathBuf;
use std::process::exit;
use std::thread;

use anyhow::bail;
use ed25519_dalek::SigningKey;
use serde::de::DeserializeOwned;
use tracing::error;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use updater::ContainerUpdater;
use updater::DEFAULT_IGNITER_IMAGE;
use updater::DEFAULT_UPDATE_INTERVAL;

use crate::api_server;
use crate::cli::CLI;
use crate::gossip;
use crate::licence_updater;
use crate::transport;

pub fn run() {
    _ = *CLI; // make sure we have the value or panic before we start

    eprintln!("Starting server: advertise address {}", CLI.config.advertise_addr);
    let cli_settings = serde_json::to_string_pretty(&*CLI).unwrap();
    eprintln!("Settings: {cli_settings}");

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
    // TODO: load state from file if possible

    let current_version = env!("CARGO_PKG_VERSION");
    let commit = env!("BUILD_GIT_COMMIT");
    info!("Starting server: version {} (commit {})", current_version, commit);

    let updater = ContainerUpdater::try_new(
        DEFAULT_IGNITER_IMAGE.to_owned(),
        DEFAULT_UPDATE_INTERVAL,
        CLI.docker_socket.clone(),
    )
    .await?;

    let updater_handle = tokio::spawn(async move { updater.run().await });

    let params = CLI.clone();

    let initial_key_values = params.to_gossip_kv()?;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());

    let (gossip_state, licence_update_handle) = {
        let udp_transport = transport::signed_udp::UdpSignedTransport::new(
            vec![],
            signing_key,
            chitchat::transport::UdpTransport,
        );

        let gossip_state = gossip::run(initial_key_values, &udp_transport).await?;

        let licence_update_handle =
            licence_updater::run_licence_updater(gossip_state.chitchat(), params.keys.clone())
                .await?;

        (gossip_state, licence_update_handle)
    };

    let rest_server_handle = tokio::spawn(api_server::run(gossip_state.chitchat()));

    tokio::select! {
        v = updater_handle => {
            anyhow::bail!("Container updater failed: {v:?}");
        }
        v = licence_update_handle => {
           anyhow::bail!("Licence updater failed: {v:?}");
        }
        v = gossip_state.join_handle => {
            anyhow::bail!("Gossip server failed: {v:?}");
        }
        v = rest_server_handle => {
            anyhow::bail!("API server failed: {v:?}");
        }
    }
}

pub fn read_config<Config: DeserializeOwned>(
    config_path: impl AsRef<Path>,
) -> anyhow::Result<Config> {
    let config_path = config_path.as_ref();
    let Some(path) = config_path.as_os_str().to_str() else {
        bail!("Invalid path {:?}", config_path);
    };
    let expanded = PathBuf::from(shellexpand::tilde(path).into_owned());
    let file = std::fs::File::open(&expanded)?;
    let config = serde_yaml::from_reader(file)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_read_failed() {
        let result = read_config::<crate::Config>(Path::new("foo"));
        assert!(result.is_err())
    }

    #[test]
    fn test_read_success() {
        let keys = read_config::<crate::Keys>(Path::new("./tests/keys.yaml"))
            .expect("File exists and valid");

        assert_eq!(
            keys.wallet.pubkey,
            "7876682d123554aeedc71eb4e437e3c25ea8c9d97c0fd3fb9521061d6f494cdc"
        );
    }
}
