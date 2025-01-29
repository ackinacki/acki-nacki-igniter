use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use chitchat::Chitchat;
use chitchat::ChitchatId;
use chitchat::ClusterStateSnapshot;
use poem_openapi::param::Query;
use poem_openapi::payload::Json;
use poem_openapi::payload::PlainText;
use poem_openapi::OpenApi;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::licence_updater::build_url;
use crate::licence_updater::check_license_number;
use crate::licence_updater::LicensesResponse;
use crate::ZerostateKeys;

pub static DEFAULT_GOSSIP_INTERVAL: Duration = Duration::from_millis(500);

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

pub struct Api {
    pub chitchat: Arc<Mutex<Chitchat>>,
}

impl Api {
    pub fn new(chitchat: Arc<Mutex<Chitchat>>) -> Self {
        Self { chitchat }
    }
}

#[OpenApi]
impl Api {
    /// Chitchat state
    #[oai(path = "/", method = "get")]
    async fn index(&self) -> PlainText<String> {
        let chitchat_guard = self.chitchat.lock().await;
        let response = ApiResponse {
            cluster_id: chitchat_guard.cluster_id().to_string(),
            cluster_state: chitchat_guard.state_snapshot(),
            live_nodes: chitchat_guard.live_nodes().cloned().collect::<Vec<_>>(),
            dead_nodes: chitchat_guard.dead_nodes().cloned().collect::<Vec<_>>(),
        };
        PlainText(serde_json::to_string_pretty(&response).unwrap())
    }

    /// Sets a key-value pair on this node (without validation).
    #[oai(path = "/set_kv/", method = "get")]
    async fn set_kv(&self, key: Query<String>, value: Query<String>) -> Json<serde_json::Value> {
        let mut chitchat_guard = self.chitchat.lock().await;

        let cc_state = chitchat_guard.self_node_state();
        cc_state.set(key.as_str(), value.as_str());

        Json(serde_json::to_value(&SetKeyValueResponse { status: true }).unwrap())
    }

    /// Export data in format applicable for zerostate.
    #[oai(path = "/export/", method = "get")]
    async fn export(&self) -> PlainText<String> {
        // Creating unverified zerostate from chitchat info
        let mut unverified_zerostate = vec![];
        {
            let chitchat_guard = self.chitchat.lock().await;

            for state in chitchat_guard.state_snapshot().node_states {
                let k_v: HashMap<String, String> =
                    state.key_values().map(|(k, v)| (k.into(), v.into())).collect();
                // insert in unverified zerostate only well formed data
                match OneHostZeroState::try_from(k_v) {
                    Ok(data) => unverified_zerostate.push(data),
                    Err(err) => tracing::error!("Skip invalid data: {}", err),
                };
            }
        }

        // Check unverified licences
        let client = Client::new();

        let mut verified_zerostate = vec![];
        for state in unverified_zerostate.clone() {
            tracing::info!("Validating license data for state {:?}", state.licenses);
            if let Ok(url) = build_url(&state.pubkey) {
                let unverified_licenses = state.licenses.inner();

                if let Ok(resp) = client.get(url).send().await {
                    if let Ok(text) = resp.text().await {
                        match check_license_number(&text) {
                            Ok(received) => {
                                let verified_licenses = received.inner();

                                if compare_maps(unverified_licenses, verified_licenses) {
                                    tracing::info!("State {:?} is included in zerostate", state);
                                    verified_zerostate.push(state.clone());
                                } else {
                                    tracing::error!(
                                        "The license is invalid, this state is NOT included in zerostate",
                                    )
                                }
                            }
                            Err(error) => {
                                tracing::error!("License valdation failed: {:?}", error);
                            }
                        }
                    } else {
                        tracing::error!("Failed to parse license");
                    }
                } else {
                    tracing::error!("Failed to fetch license");
                }
            } else {
                tracing::error!("Failed to build URL");
            }
        }

        PlainText(
            serde_json::to_string_pretty(&verified_zerostate).expect("Serialization can't fail"),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OneHostZeroState {
    pub pubkey: String,
    pub bls_key: String,
    pub proxies: HashMap<usize, String>,
    pub licenses: LicensesResponse,
}

impl TryFrom<HashMap<String, String>> for OneHostZeroState {
    type Error = anyhow::Error;

    fn try_from(one_section: HashMap<String, String>) -> anyhow::Result<Self> {
        let pubkey = one_section
            .get(&ZerostateKeys::Pubkey.to_string())
            .ok_or_else(|| anyhow!("Missing required field: pubkey"))?
            .to_string();

        let bls_key = one_section
            .get(&ZerostateKeys::BlsPubkey.to_string())
            .ok_or_else(|| anyhow!("Missing required field: bls_pubkey"))?
            .to_string();

        let proxies_vec: Vec<crate::ProxyConfig> = serde_json::from_str(
            one_section
                .get(&ZerostateKeys::Proxies.to_string())
                .ok_or_else(|| anyhow!("Missing required field: proxies"))?,
        )?;

        let mut proxies: HashMap<usize, String> = HashMap::new();
        for (k, v) in proxies_vec.iter().enumerate() {
            let value = format!("{} {}", v.url, v.cert);
            proxies.insert(k, value);
        }

        let licenses: LicensesResponse = serde_json::from_str(
            one_section
                .get(&ZerostateKeys::Licenses.to_string())
                .ok_or_else(|| anyhow!("Missing required field: licenses"))?,
        )?;

        Ok(OneHostZeroState { pubkey, bls_key, proxies, licenses })
    }
}

/// Compare two hash maps for equality.
fn compare_maps<K, V>(map1: &HashMap<K, V>, map2: &HashMap<K, V>) -> bool
where
    K: Eq + Hash,
    V: PartialEq,
{
    if map1.len() != map2.len() {
        return false;
    }
    for (key, val1) in map1 {
        match map2.get(key) {
            Some(val2) if val1 == val2 => continue,
            Some(_) => {
                return false;
            }
            None => {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_maps() -> (HashMap<String, u32>, HashMap<String, u32>) {
        let mut map1 = HashMap::new();
        let mut map2 = HashMap::new();

        map1.insert("a".to_string(), 1);
        map1.insert("b".to_string(), 2);

        map2.insert("a".to_string(), 1);
        map2.insert("b".to_string(), 2);

        (map1, map2)
    }

    #[test]
    fn test_equal_maps() {
        let (map1, map2) = create_test_maps();
        assert!(compare_maps(&map1, &map2));
    }

    #[test]
    fn test_different_values() {
        let (map1, mut map2) = create_test_maps();
        map2.insert("b".to_string(), 3);

        assert!(!compare_maps(&map1, &map2));
    }

    #[test]
    fn test_different_keys() {
        let (mut map1, map2) = create_test_maps();
        map1.insert("c".to_string(), 3);

        assert!(!compare_maps(&map1, &map2));
    }

    #[test]
    fn test_generic_compare_maps() {
        let mut map1 = HashMap::new();
        let mut map2 = HashMap::new();

        map1.insert(1, "one");
        map2.insert(1, "one");

        assert!(compare_maps(&map1, &map2));

        map2.insert(2, "two");
        assert!(!compare_maps(&map1, &map2));
    }
}
