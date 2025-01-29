use cli::Params;
use serde::Deserialize;
use serde::Serialize;
use strum::Display;
use strum::EnumString;

pub mod api;
pub mod api_server;
pub mod cli;
pub mod gossip;
mod licence_updater;
pub mod process;
pub mod transport;

#[derive(Debug, Clone, Copy, Display, EnumString, PartialEq, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum ZerostateKeys {
    Pubkey,
    BlsPubkey,
    Proxies,
    Licenses,
    Version,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keys {
    pub wallet: WalletConfig,
    pub bls: BlsConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub proxies: Vec<ProxyConfig>,

    // Defines the socket addr on which we should listen to, default_value = "0.0.0.0:10000"
    #[serde(deserialize_with = "deserialize_addr", default = "default_listen_addr")]
    pub listen_addr: SocketAddr,

    //  Defines the socket addr on which the API should listen to. default_value = "0.0.0.0:10001"
    #[serde(deserialize_with = "deserialize_addr", default = "default_api_addr")]
    pub api_addr: SocketAddr,

    // Defines the socket address (host:port) other servers should use to
    // reach this server. Defaults to "127.0.0.1:10000"
    #[serde(deserialize_with = "deserialize_addr", default = "default_advertise_addr")]
    pub advertise_addr: SocketAddr,

    #[serde(default)]
    pub seeds: Vec<String>,

    #[serde(default)]
    pub node_id: Option<String>,

    #[serde(default = "default_interval")]
    pub interval: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalletConfig {
    pub pubkey: String,
    pub secret: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlsConfig {
    pub pubkey: String,
    pub secret: String,
    pub rnd: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProxyConfig {
    pub url: String,
    pub cert: String,
}

impl Params {
    pub fn to_gossip_kv(&self) -> Result<Vec<(String, String)>, serde_json::Error> {
        Ok([
            (ZerostateKeys::Pubkey.to_string(), self.keys.wallet.pubkey.clone()),
            (ZerostateKeys::BlsPubkey.to_string(), self.keys.bls.pubkey.clone()),
            (ZerostateKeys::Proxies.to_string(), serde_json::to_string(&self.config.proxies)?),
            (ZerostateKeys::Version.to_string(), env!("CARGO_PKG_VERSION").to_string()),
        ]
        .to_vec())
    }
}
fn default_interval() -> u64 {
    500
}

fn default_listen_addr() -> SocketAddr {
    "0.0.0.0:10000".parse().expect("Invalid default address")
}

fn default_api_addr() -> SocketAddr {
    "0.0.0.0:10001".parse().expect("Invalid default address")
}
fn default_advertise_addr() -> SocketAddr {
    "127.0.0.1:8080".parse().expect("Invalid default address")
}
use std::net::SocketAddr;

fn deserialize_addr<'de, D>(deserializer: D) -> Result<SocketAddr, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let addr_str: String = String::deserialize(deserializer)?;
    addr_str.parse::<SocketAddr>().map_err(serde::de::Error::custom)
}
