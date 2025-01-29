use std::path::PathBuf;
use std::sync::LazyLock;

use crate::process::read_config;
use crate::{Config, Keys};
use clap::Parser;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Params {
    pub config: Config,
    pub docker_socket: Option<String>,
    pub keys: Keys,
}

/// Cli args are globaly accessible for convenience
pub static CLI: LazyLock<Params> = LazyLock::new(|| {
    let cli: CliArgs = CliArgs::parse();
    let config = match read_config::<Config>(cli.config) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{}", error);
            std::process::exit(1);
        }
    };

    let keys = match read_config::<Keys>(cli.keys) {
        Ok(keys) => keys,
        Err(error) => {
            eprintln!("{}", error);
            std::process::exit(1);
        }
    };
    Params { config, docker_socket: cli.docker_socket, keys }
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
}
