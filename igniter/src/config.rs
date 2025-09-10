use std::net::ToSocketAddrs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::bail;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;

lazy_static::lazy_static! {
    pub static ref DEV_MODE: bool = {
        std::env::var("DEV_MODE")
            .map(|val| val.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    };

    pub static ref BACKEND_VERIFYING_KEY:  &'static str = {
        if *DEV_MODE {
            println!("Running in dev mode");
            "ee99af158c8b50f6bee3360615e08db957bd898568638f308d8f320cf4e37638"
        } else {
            println!("Running in production mode");
            "75631f108a226740a8649ff7946bf19d2884c373615d27f5b6d1863b5d97adf3"
        }
    };
    pub static ref IGNITER_IMAGE:  &'static str = {
        if *DEV_MODE {
            "docker.gosh.sh/acki-nacki-igniter-pre-release"
        } else {
            "teamgosh/acki-nacki-igniter"
        }
    };

    pub static ref IGNITER_SEEDS:  &'static str = {
        if *DEV_MODE {
            "https://raw.githubusercontent.com/gosh-sh/acki-nacki-igniter-seeds/refs/heads/main/seeds.yaml"
        } else {
            "https://raw.githubusercontent.com/ackinacki/acki-nacki-igniter-seeds/refs/heads/main/seeds.yaml"
        }
    };

}

macro_rules! hide_secrets_fmt {
    ($self:ident, $f:ident, $name:literal, [$($field:ident),*]) => {
        write!(
            $f,
            "{} {{ {} }}",
            $name,
            vec![
                $(
                    if stringify!($field) == "secret" || stringify!($field).contains("rnd") {
                        format!("{}: <hidden>", stringify!($field))
                    } else {
                        format!("{}: {:?}", stringify!($field), $self.$field)
                    }
                ),*
            ].join(", ")
        )
    };
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    // Chitchat cluster id for gossip
    #[serde(default = "default_cluster_id")]
    pub cluster_id: String,

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

    #[serde(default)]
    pub signatures: Vec<LicenceSignature>,

    pub auto_update: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keys {
    pub wallet: WalletConfig,
    pub bls: BlsConfig,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WalletConfig {
    pub pubkey: String,
    pub secret: String,
}
impl std::fmt::Debug for WalletConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        hide_secrets_fmt!(self, f, "WalletConfig", [pubkey, secret])
    }
}

impl std::fmt::Display for WalletConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        hide_secrets_fmt!(self, f, "WalletConfig", [pubkey, secret])
    }
}
impl std::fmt::Debug for BlsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        hide_secrets_fmt!(self, f, "BlsConfig", [pubkey, secret, rnd])
    }
}

impl std::fmt::Display for BlsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        hide_secrets_fmt!(self, f, "BlsConfig", [pubkey, secret, rnd])
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BlsConfig {
    pub pubkey: String,
    pub secret: String,
    pub rnd: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProxyConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_address: Option<SocketAddr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LicenceSignature {
    pub license_id: String,
    pub license_owner_pubkey: String,
    pub provider_pubkey: String,
    pub delegation_sig: String,
    pub delegation_confirm_sig: String,
    pub timestamp: u64,
    pub license_proof_sig: String,
}
impl HasTimestampAndId for LicenceSignature {
    fn get_id(&self) -> String {
        self.license_id.clone()
    }

    fn get_timestamp(&self) -> u64 {
        self.timestamp
    }
}

pub fn read_yaml<T: DeserializeOwned>(config_path: impl AsRef<Path>) -> anyhow::Result<T> {
    let config_path = config_path.as_ref();
    let Some(path) = config_path.as_os_str().to_str() else {
        bail!("Invalid path {:?}", config_path);
    };
    let expanded = PathBuf::from(shellexpand::tilde(path).into_owned());
    let file = std::fs::File::open(&expanded)?;
    let config = serde_yaml::from_reader(file)?;
    Ok(config)
}

fn default_interval() -> u64 {
    500
}

fn default_cluster_id() -> String {
    env!("CARGO_PKG_NAME").to_string()
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

use crate::utils::HasTimestampAndId;

fn deserialize_addr<'de, D>(deserializer: D) -> Result<SocketAddr, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let addr_str: String = String::deserialize(deserializer)?;
    let result = addr_str.parse::<SocketAddr>().map_err(serde::de::Error::custom);
    if result.is_err() {
        // try to resolve addr_str as a SockerAddr
        if let Ok(mut sockets) = addr_str.to_socket_addrs() {
            if let Some(socket) = sockets.next() {
                return Ok(socket);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_config_failed() {
        let result = read_yaml::<Config>(Path::new("foo"));
        assert!(result.is_err())
    }

    #[test]
    fn read_config_success() {
        let keys =
            read_yaml::<Keys>(Path::new("./tests/keys.yaml")).expect("File exists and valid");

        assert_eq!(
            keys.wallet.pubkey,
            "7876682d123554aeedc71eb4e437e3c25ea8c9d97c0fd3fb9521061d6f494cdc"
        );
        let cfg =
            read_yaml::<Config>(Path::new("./tests/config.yaml")).expect("File exists and valid");

        assert_eq!(cfg.signatures[0].license_id, "license_id_0");
    }

    #[test]
    fn read_config_no_proxies_success() {
        let cfg = read_yaml::<Config>(Path::new("./tests/config-no-proxies.yaml"))
            .expect("File exists and valid");
        assert_eq!(cfg.signatures[0].license_id, "license_id_0");
    }
    #[test]
    fn read_config_invalid_proxies_failed() {
        match read_yaml::<Config>(Path::new("./tests/config-invalid-proxies.yaml")) {
            Ok(cfg) => {
                panic!("Expected error, but got config: {cfg:?}");
            }
            Err(e) => {
                assert!(e.to_string().contains("invalid socket address"));
            }
        }
    }
}
