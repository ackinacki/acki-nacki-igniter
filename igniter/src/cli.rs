use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::bail;
use clap::Parser;
use reqwest;
use reqwest::blocking::Client;
use serde::Serialize;
use serde_yaml;

use crate::config::read_yaml;
use crate::config::Config;
use crate::config::Keys;
use crate::config::DEV_MODE;
use crate::config::IGNITER_SEEDS;

#[derive(Debug, Clone, Serialize)]
pub struct Params {
    pub config: Config,
    pub docker_socket: Option<String>,
    pub docker_config: Option<String>,
    pub keys: Keys,
}

/// Cli args are globaly accessible for convenience
pub static CLI: LazyLock<Params> = LazyLock::new(|| {
    let cli: CliArgs = CliArgs::parse();
    let mut config = match read_yaml::<Config>(&cli.config) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Error parsing config file {:?}: {:?}", cli.config, error);
            std::process::exit(1);
        }
    };

    config.seeds = match read_seeds(&IGNITER_SEEDS) {
        Ok(seeds) => {
            seeds
            // vec!["127.0.0.1:10000".to_string(), "127.0.0.1:10001".to_string()]
        }
        Err(error) => {
            eprintln!(
                "Initialization error: unable to download seeds from {} {error}",
                *IGNITER_SEEDS
            );
            std::process::exit(1);
        }
    };

    //

    let keys = match read_yaml::<Keys>(&cli.keys) {
        Ok(keys) => keys,
        Err(error) => {
            eprintln!("Error parsing keys file {:?}: {:?}", cli.keys, error);
            std::process::exit(1);
        }
    };
    Params { config, docker_socket: cli.docker_socket, docker_config: cli.docker_config, keys }
});

pub static LONG_VERSION: LazyLock<String> = LazyLock::new(|| {
    format!(
        "
{}
BUILD_GIT_BRANCH={}
BUILD_GIT_COMMIT={}
BUILD_GIT_DATE={}
BUILD_TIME={}",
        env!("CARGO_PKG_VERSION"),
        env!("BUILD_GIT_BRANCH"),
        env!("BUILD_GIT_COMMIT"),
        env!("BUILD_GIT_DATE"),
        env!("BUILD_TIME"),
    )
});

/// Acki Nacki Gossip Igniter
#[derive(Parser, Debug, Clone, Serialize)]
#[command(author, long_version = &**LONG_VERSION, about, long_about = None)]
pub struct CliArgs {
    #[arg(short, long)]
    pub keys: PathBuf,

    #[arg(short, long)]
    pub config: PathBuf,

    /// host's docker UNIX socket
    #[arg(long, env, default_value = "/var/run/docker.sock")]
    pub docker_socket: Option<String>,

    /// host's docker config
    #[arg(long, env)]
    pub docker_config: Option<String>,
}

fn read_seeds(url: &str) -> anyhow::Result<Vec<String>> {
    let client = Client::new();
    let mut request = client.get(url);
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        request = request.bearer_auth(token);
    } else if *DEV_MODE {
        bail!("GITHUB_TOKEN required")
    }
    let body = request.send()?.text()?;
    let seeds: Vec<String> = serde_yaml::from_str(&body)?;
    Ok(seeds)
}
