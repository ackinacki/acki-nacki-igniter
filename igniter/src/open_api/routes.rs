use std::collections::HashMap;
use std::time::Duration;
use std::vec;

use anyhow::anyhow;
use chitchat::ChitchatId;
use chitchat::ChitchatRef;
use chitchat::ClusterStateSnapshot;
use poem_openapi::payload::PlainText;
use poem_openapi::OpenApi;
use serde::Deserialize;
use serde::Serialize;

use crate::config::LicenceSignature;
use crate::config::ProxyConfig;
use crate::ZerostateKeys;
use crate::BACKEND_VERIFYING_KEY;

pub static DEFAULT_GOSSIP_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse {
    pub cluster_id: String,
    pub cluster_state: ClusterStateSnapshot,
    pub live_nodes: Vec<ChitchatId>,
    pub dead_nodes: Vec<ChitchatId>,
}

pub struct Api {
    pub chitchat: ChitchatRef,
}

impl Api {
    pub fn new(chitchat: ChitchatRef) -> Self {
        Self { chitchat }
    }
}

#[OpenApi]
impl Api {
    /// Chitchat state
    #[oai(path = "/", method = "get")]
    async fn index(&self) -> PlainText<String> {
        let (cluster_id, live_nodes, dead_nodes, mut state_snapshot) = {
            let chitchat_guard = self.chitchat.lock();
            (
                chitchat_guard.cluster_id().to_string(),
                chitchat_guard.live_nodes().cloned().collect::<Vec<_>>(),
                chitchat_guard.dead_nodes().cloned().collect::<Vec<_>>(),
                chitchat_guard.state_snapshot(),
            )
        };

        // Parse each node_state to check that:
        // 1. it has all required properties
        // 2. signaturesare valid
        // 3. signatures  and `licences` match
        // 4. all signatures are unique
        // TODO: We do not validate the `proxies` property, should we?
        state_snapshot.node_states.retain(|node_state| {
            let k_v: HashMap<String, String> =
                node_state.key_values().map(|(k, v)| (k.into(), v.into())).collect();

            match VerifiedNodeState::from_gossip(k_v.clone()) {
                Ok(verified) => match validate_licenses(&k_v, &verified.licenses) {
                    Ok(_) => true,
                    Err(err) => {
                        tracing::error!("Skip invalid data: {}", err);
                        false
                    }
                },
                Err(error) => {
                    tracing::error!("Gossip node state can't be parsed: {:?}", error);
                    false
                }
            }
        });

        let res = ApiResponse { cluster_id, cluster_state: state_snapshot, live_nodes, dead_nodes };

        PlainText(
            serde_json::to_string_pretty(&res).expect("Serialization of ApiResponse cannot fail"),
        )
    }

    /// Export data in format applicable for zerostate.
    #[oai(path = "/export", method = "get")]
    async fn export(&self) -> PlainText<String> {
        // Creating unverified zerostate from chitchat info
        let mut zerostate = vec![];
        {
            let chitchat_guard = self.chitchat.lock();

            for state in chitchat_guard.state_snapshot().node_states {
                let k_v: HashMap<String, String> = state
                    .key_values() // returns  only non-deleted key-values
                    .map(|(k, v)| (k.into(), v.into())).collect();

                match VerifiedNodeState::from_gossip(k_v) {
                    Ok(data) => zerostate.push(data),
                    Err(err) => tracing::error!("Skip invalid data: {:?}", err),
                };
            }
        }

        PlainText(serde_json::to_string_pretty(&zerostate).expect("Serialization can't fail"))
    }
}

// Validates the `licenses` property in the gossip state.
fn validate_licenses(
    k_v: &HashMap<String, String>,
    verified_licenses: &HashMap<String, i32>,
) -> Result<(), &'static str> {
    let json_str = k_v
        .get(&ZerostateKeys::Licenses.to_string())
        .ok_or("\"licenses\" property is missing in gossip")?;

    let map: HashMap<String, i32> =
        serde_json::from_str(json_str).map_err(|_| "\"licenses\" property must be a HashMap")?;

    if map != *verified_licenses {
        return Err("\"licenses\" and \"signatures\" properties do not match");
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedNodeState {
    pub pubkey: String,
    pub bls_key: String,
    pub proxies: HashMap<usize, String>,
    pub signatures: Vec<LicenceSignature>,
    pub licenses: HashMap<String, i32>,
    pub version: String,
}

impl VerifiedNodeState {
    fn from_gossip(section: HashMap<String, String>) -> anyhow::Result<Self> {
        let pubkey = section
            .get(&ZerostateKeys::Pubkey.to_string())
            .ok_or_else(|| anyhow!("Missing required field: pubkey"))?
            .to_string();

        let bls_key = section
            .get(&ZerostateKeys::BlsPubkey.to_string())
            .ok_or_else(|| anyhow!("Missing required field: bls_pubkey"))?
            .to_string();

        let proxies_vec: Vec<ProxyConfig> = serde_json::from_str(
            section
                .get(&ZerostateKeys::Proxies.to_string())
                .ok_or_else(|| anyhow!("Missing required field: proxies"))?,
        )?;

        let mut proxies: HashMap<usize, String> = HashMap::new();
        for (k, v) in proxies_vec.iter().enumerate() {
            let value = format!("{} {}", v.url, v.cert);
            proxies.insert(k, value);
        }

        let signatures_as_string = section
            .get(&ZerostateKeys::Signatures.to_string())
            .ok_or_else(|| anyhow!("Missing required field: signatures"))?;

        let signatures: Vec<LicenceSignature> = serde_json::from_str(signatures_as_string)?;

        LicenceSignature::check_all_signatures_in_section(
            &signatures,
            &BACKEND_VERIFYING_KEY,
            &pubkey,
            &bls_key,
        )?;

        let licenses = LicenceSignature::derive_licences(&signatures);

        let version = section
            .get(&ZerostateKeys::Version.to_string())
            .ok_or_else(|| anyhow!("Missing required field: version"))?
            .to_string();

        Ok(VerifiedNodeState { pubkey, bls_key, proxies, signatures, licenses, version })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_licenses_success() {
        let mut gossip_data = HashMap::new();
        let verified_licenses = HashMap::from([("owner_key".to_string(), 10)]);

        gossip_data.insert(
            ZerostateKeys::Licenses.to_string(),
            serde_json::to_string(&verified_licenses).unwrap(),
        );

        assert!(validate_licenses(&gossip_data, &verified_licenses).is_ok());
    }

    #[test]
    fn test_validate_licenses_missing_key() {
        let gossip_data = HashMap::new();
        let verified_licenses = HashMap::from([("owner_key".to_string(), 10)]);

        let result = validate_licenses(&gossip_data, &verified_licenses);
        assert_eq!(result, Err("\"licenses\" property is missing in gossip"));
    }

    #[test]
    fn test_validate_licenses_invalid_json() {
        let mut gossip_data = HashMap::new();
        gossip_data.insert(ZerostateKeys::Licenses.to_string(), "not a json".to_string());

        let verified_licenses = HashMap::from([("owner_key".to_string(), 10)]);

        let result = validate_licenses(&gossip_data, &verified_licenses);
        assert_eq!(result, Err("\"licenses\" property must be a HashMap"));
    }

    #[test]
    fn test_validate_licenses_mismatch() {
        let mut gossip_data = HashMap::new();
        let verified_licenses = HashMap::from([("owner_key".to_string(), 10)]);
        let gossip_licenses = HashMap::from([("owner_key".to_string(), 5)]);

        gossip_data.insert(
            ZerostateKeys::Licenses.to_string(),
            serde_json::to_string(&gossip_licenses).unwrap(),
        );

        let result = validate_licenses(&gossip_data, &verified_licenses);
        assert_eq!(result, Err("\"licenses\" and \"signatures\" properties do not match"));
    }
}
