use std::collections::HashMap;
use std::time::Duration;
use std::vec;

use anyhow::anyhow;
use chitchat::ChitchatId;
use chitchat::ChitchatRef;
use chitchat::ClusterStateSnapshot;
use chitchat::NodeState;
use poem_openapi::param::Query;
use poem_openapi::payload::PlainText;
use poem_openapi::OpenApi;
use serde::Deserialize;
use serde::Serialize;

use crate::config::LicenceSignature;
use crate::config::ProxyConfig;
use crate::utils::remove_with_outdated_timestamps;
use crate::utils::ContainsVec;
use crate::utils::RevokedLicense;
use crate::VerifiedSignatures;
use crate::ZerostateKeys;
use crate::BACKEND_VERIFYING_KEY;

pub static DEFAULT_GOSSIP_INTERVAL: Duration = Duration::from_millis(500);

pub const MAX_PROXIES: usize = 10;

pub fn extract_verified_state_without_licences(
    node_states: Vec<NodeState>,
) -> (Vec<VerifiedNodeState>, Vec<RevokedLicense>) {
    let mut verified_state_without_licences = vec![];
    for state in node_states {
        // Create hashmap from non-deleted key-values
        let k_v: HashMap<String, String> =
            state.key_values().map(|(k, v)| (k.into(), v.into())).collect();

        match VerifiedNodeStateNoLicenses::from_gossip(k_v) {
            Ok(data) => verified_state_without_licences.push(data),
            Err(err) => tracing::error!("Skip invalid data: {:?}", err),
        };
    }
    let (verified_state, revoked_licenses) =
        VerifiedNodeState::from_state(verified_state_without_licences);

    (verified_state, revoked_licenses)
}

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

    pub fn get_verified_state(&self) -> (Vec<VerifiedNodeState>, Vec<RevokedLicense>) {
        let node_states = self.chitchat.lock().state_snapshot().node_states;
        extract_verified_state_without_licences(node_states)
    }
}

#[OpenApi]
impl Api {
    /// Chitchat state
    #[oai(path = "/", method = "get")]
    async fn index(&self) -> PlainText<String> {
        // We need verified state to compare derived licenses with the licenses in the current state
        let (verified_state, _) = self.get_verified_state();

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
        // 2. signatures are valid
        // 3. signatures and `licences` match
        // 4. Check that proxies contains valid socket addresses
        state_snapshot.node_states.retain(|node_state| {
            let k_v: HashMap<String, String> =
                node_state.key_values().map(|(k, v)| (k.into(), v.into())).collect();

            // Check that node_state has all required properties
            match VerifiedNodeStateNoLicenses::from_gossip(k_v.clone()) {
                Ok(VerifiedNodeStateNoLicenses { pubkey, .. }) => {
                    let derived_licences =
                        match verified_state.iter().find(|s| s.pubkey == pubkey) {
                            Some(x) => x.licenses.clone(),
                            None => {
                                tracing::error!("Skip node with pubkey {pubkey}. It is not included in verified state");
                                return false;
                            }
                        };

                    if let Err(err) = validate_licenses(&k_v, &derived_licences) {
                        tracing::error!(
                            "Skip node with pubkey {pubkey}. It provides invalid licenses info: {err}",
                        );
                        return false;
                    };

                    if !check_proxy_socket_addresses(k_v.get(&ZerostateKeys::Proxies.to_string())) {
                        tracing::error!(
                            "Skip node with pubkey {pubkey}. It provides invalid proxy info",
                        );
                        return false;
                    }
                    true
                }
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

    /// returns all licenses that have been re-delegated to another node
    #[oai(path = "/getRevokedLicenses", method = "get")]
    async fn get_revoked_licenses(&self, provider_pubkey: Query<String>) -> PlainText<String> {
        let pubkey = provider_pubkey.0;

        let (_, revoked_licenses) = self.get_verified_state();
        let your_revoked_licenses: Vec<RevokedLicense> =
            revoked_licenses.into_iter().filter(|elem| elem.provider_pubkey == pubkey).collect();

        if your_revoked_licenses.is_empty() {
            return PlainText(
                serde_json::to_string_pretty(&"No revoked licenses found")
                    .expect("Serialization can't fail"),
            );
        }
        PlainText(
            serde_json::to_string_pretty(&your_revoked_licenses).expect("Serialization can't fail"),
        )
    }

    /// Export data to create zerostate
    #[oai(path = "/export", method = "get")]
    async fn export(&self) -> PlainText<String> {
        let (verified_state, _) = self.get_verified_state();
        PlainText(serde_json::to_string_pretty(&verified_state).expect("Serialization can't fail"))
    }
}

pub fn check_proxy_socket_addresses(str_value: Option<&String>) -> bool {
    match str_value {
        Some(str_value) => match serde_json::from_str::<Vec<ProxyConfig>>(str_value) {
            Ok(records) => {
                if records.len() > MAX_PROXIES {
                    return false;
                }
                let mut is_ok = true;
                // check that if cert exists, socket_address exists too
                for ProxyConfig { socket_address, cert } in records {
                    if cert.is_some() && socket_address.is_none() {
                        is_ok = false;
                        break;
                    }
                }
                is_ok
            }
            Err(_) => false,
        },
        None => true,
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
pub struct VerifiedNodeStateNoLicenses {
    pubkey: String,
    bls_key: String,
    signatures: Vec<LicenceSignature>,
    version: String,
}

impl VerifiedNodeStateNoLicenses {
    pub fn get_signatures(&self) -> Vec<LicenceSignature> {
        self.signatures.clone()
    }

    fn from_gossip(section: HashMap<String, String>) -> anyhow::Result<Self> {
        let pubkey = section
            .get(&ZerostateKeys::Pubkey.to_string())
            .ok_or_else(|| anyhow!("Missing required field: pubkey"))?
            .to_string();

        let bls_key = section
            .get(&ZerostateKeys::BlsPubkey.to_string())
            .ok_or_else(|| anyhow!("Missing required field: bls_pubkey"))?
            .to_string();

        let signatures: Vec<LicenceSignature> = serde_json::from_str(
            section
                .get(&ZerostateKeys::Signatures.to_string())
                .ok_or_else(|| anyhow!("Missing required field: signatures"))?,
        )?;

        let verified_signatures =
            VerifiedSignatures::create(&signatures, &BACKEND_VERIFYING_KEY, &pubkey, &bls_key)?;

        let version = section
            .get(&ZerostateKeys::Version.to_string())
            .ok_or_else(|| anyhow!("Missing required field: version"))?
            .to_string();
        Ok(VerifiedNodeStateNoLicenses {
            pubkey,
            bls_key,
            signatures: verified_signatures.get().clone(),
            version,
        })
    }
}

impl ContainsVec<LicenceSignature> for VerifiedNodeStateNoLicenses {
    fn get_mut_vec(&mut self) -> &mut Vec<LicenceSignature> {
        &mut self.signatures
    }

    fn is_empty(&self) -> bool {
        self.signatures.is_empty()
    }

    fn get_pk(&self) -> String {
        self.pubkey.clone()
    }
}
pub struct Licences {
    inner: HashMap<String, i32>,
}
impl Licences {
    // Generate `licenses` from `signatures`
    pub fn derive_licences(signatures: &VerifiedSignatures) -> Licences {
        let hm = signatures.get().iter().fold(HashMap::new(), |mut acc, sig| {
            *acc.entry(sig.license_owner_pubkey.to_string()).or_insert(0) += 1;
            acc
        });
        Licences { inner: hm }
    }

    pub fn get(&self) -> HashMap<String, i32> {
        self.inner.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedNodeState {
    pubkey: String,
    bls_key: String,
    signatures: Vec<LicenceSignature>,
    licenses: HashMap<String, i32>,
    version: String,
}

impl VerifiedNodeState {
    pub fn from_state(state: Vec<VerifiedNodeStateNoLicenses>) -> (Vec<Self>, Vec<RevokedLicense>) {
        let (_, problem) = remove_with_outdated_timestamps(state.clone());

        let mut verified_state = vec![];

        for node_state in state.iter() {
            let checked_signatures = VerifiedSignatures::from_checked_state(node_state);
            let licenses = Licences::derive_licences(&checked_signatures);

            verified_state.push(Self {
                pubkey: node_state.pubkey.clone(),
                bls_key: node_state.bls_key.clone(),
                signatures: node_state.signatures.clone(),
                version: node_state.version.clone(),
                licenses: licenses.inner,
            });
        }

        (verified_state, problem)
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

    #[test]
    fn check_proxy() {
        let proxies = serde_json::to_string(&serde_json::json!([
            {"socket_address": "127.0.0.1:8080"}]))
        .unwrap();

        assert!(check_proxy_socket_addresses(Some(&proxies)));
    }
    #[test]
    fn check_many_proxies() {
        let proxies = serde_json::to_string(&serde_json::json!([
            {"socket_address": "127.0.0.1:8080", "cert": "my_cert_1" },
            {"socket_address": "127.0.0.3:8080", "cert": "my_cert_2" }
        ]))
        .unwrap();
        assert!(check_proxy_socket_addresses(Some(&proxies)));
    }

    #[test]
    fn check_proxies_address_fail() {
        let proxies = serde_json::to_string(&serde_json::json!([
            {"socket_address": "a.b.0.1:8080", "cert": "my_cert_1" },
            {"socket_address": "127.0.0.3:8080", "cert": "my_cert_2" }
        ]))
        .unwrap();
        assert!(!check_proxy_socket_addresses(Some(&proxies)));
    }
    #[test]
    fn check_proxies_fail() {
        let proxies = serde_json::to_string(&serde_json::json!([
            {"socket_address": "127.0.0.1:8080", "cert": "my_cert_1" },
            {"cert": "my_cert_2" }
        ]))
        .unwrap();
        assert!(!check_proxy_socket_addresses(Some(&proxies)));
    }

    #[test]
    fn check_to_many_proxies_fail() {
        let proxies = serde_json::to_string(&serde_json::json!([
            {"socket_address": "127.0.0.1:8080", "cert": "my_cert_1" },
            {"socket_address": "127.0.0.2:8080", "cert": "my_cert_1" },
            {"socket_address": "127.0.0.3:8080", "cert": "my_cert_1" },
            {"socket_address": "127.0.0.4:8080", "cert": "my_cert_1" },
            {"socket_address": "127.0.0.5:8080", "cert": "my_cert_1" },
            {"socket_address": "127.0.0.6:8080", "cert": "my_cert_1" },
            {"socket_address": "127.0.0.7:8080", "cert": "my_cert_1" },
            {"socket_address": "127.0.0.8:8080", "cert": "my_cert_1" },
            {"socket_address": "127.0.0.9:8080", "cert": "my_cert_1" },
            {"socket_address": "127.0.0.10:8080", "cert": "my_cert_1" },
            {"socket_address": "127.0.0.11:8080", "cert": "my_cert_1" },
        ]))
        .unwrap();
        assert!(!check_proxy_socket_addresses(Some(&proxies)));
    }
}
