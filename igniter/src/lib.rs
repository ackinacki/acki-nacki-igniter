use cli::Params;
use config::LicenceSignature;
use config::BACKEND_VERIFYING_KEY;
pub use config::IGNITER_IMAGE;
use errors::IgniterError;
use serde::Deserialize;
use serde::Serialize;
use strum::Display;
use strum::EnumString;
use tvm_types::ed25519_verify;
pub mod cli;
mod config;
pub mod errors;
pub mod gossip;
pub mod open_api;
pub mod transport;

use std::collections::HashMap;
use std::collections::HashSet;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;

pub const MAX_ALLOWED_LICENSES: u32 = 10;

#[derive(Debug, Clone, Copy, Display, EnumString, PartialEq, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum ZerostateKeys {
    Pubkey,
    BlsPubkey,
    Proxies,
    Licenses,
    Signatures,
    Version,
}

impl Params {
    pub fn to_gossip(&self) -> Result<Vec<(String, String)>, IgniterError> {
        let mut keys = [
            (ZerostateKeys::Pubkey.to_string(), self.keys.wallet.pubkey.clone()),
            (ZerostateKeys::BlsPubkey.to_string(), self.keys.bls.pubkey.clone()),
            (ZerostateKeys::Proxies.to_string(), serde_json::to_string(&self.config.proxies)?),
            (ZerostateKeys::Version.to_string(), env!("CARGO_PKG_VERSION").to_string()),
        ]
        .to_vec();

        let signatures = self.config.signatures.clone();

        LicenceSignature::check_all_signatures_in_section(
            &signatures,
            &BACKEND_VERIFYING_KEY,
            &self.keys.wallet.pubkey,
            &self.keys.bls.pubkey,
        )?;

        keys.push((ZerostateKeys::Signatures.to_string(), serde_json::to_string(&signatures)?));

        let licenses = LicenceSignature::derive_licences(&signatures);
        keys.push((ZerostateKeys::Licenses.to_string(), serde_json::to_string(&licenses)?));

        Ok(keys)
    }
}

impl LicenceSignature {
    // These functions concatenate values into a string that will be signed.
    fn license_proof_prepare(license_id: &str, license_owner_pubkey: &str) -> Vec<u8> {
        format!("{}{}", license_id, license_owner_pubkey).into_bytes()
    }

    fn delegation_prepare(
        license_id: &str,
        license_owner_pubkey: &str,
        provider_pubkey: &str,
        timestamp: u64,
    ) -> Vec<u8> {
        format!("{}{}{}{}", license_owner_pubkey, provider_pubkey, license_id, timestamp)
            .into_bytes()
    }

    fn delegation_confirm_prepare(
        license_id: &str,
        license_owner_pubkey: &str,
        provider_pubkey: &str,
        bk_node_owner_pubkey: &str,
        bk_bls_pubkey: &str,
    ) -> Vec<u8> {
        format!(
            "{}{}{}{}{}",
            license_id, license_owner_pubkey, provider_pubkey, bk_node_owner_pubkey, bk_bls_pubkey
        )
        .into_bytes()
    }

    fn check_license_proof_sig(&self, backend_pk: &[u8; 32]) -> Result<(), IgniterError> {
        let message = Self::license_proof_prepare(&self.license_id, &self.license_owner_pubkey);
        let signature =
            STANDARD.decode(&self.license_proof_sig).map_err(|_| IgniterError::LicenseProofSig)?;
        ed25519_verify(backend_pk, &message, &signature).map_err(|_| IgniterError::LicenseProofSig)
    }

    // delegation_sig: check data {license_id, license_owner_pubkey,provider_pubkey, ​​timestamp } with license_owner_pubkey
    fn check_delegation_sig(&self) -> Result<(), IgniterError> {
        let check = || -> anyhow::Result<()> {
            let data = Self::delegation_prepare(
                &self.license_id,
                &self.license_owner_pubkey,
                &self.provider_pubkey,
                self.timestamp,
            );

            let signature = STANDARD.decode(&self.delegation_sig)?;
            let pub_key = hex::decode(&self.license_owner_pubkey)?;
            let pub_key: &[u8; 32] = pub_key.as_slice().try_into()?;

            ed25519_verify(pub_key, &data, &signature).map_err(|_| IgniterError::DelegationSig)?;
            Ok(())
        };
        check().map_err(|_| IgniterError::DelegationSig)
    }

    // delegation_confirm_sig : check data {license_id,license_owner_pubkey, provider_pubkey } with provider_pubkey
    fn check_delegation_confirm_sig(
        &self,
        bk_node_owner_pubkey: &str,
        bk_bls_pubkey: &str,
    ) -> Result<(), IgniterError> {
        let check = || -> anyhow::Result<()> {
            let data = Self::delegation_confirm_prepare(
                &self.license_id,
                &self.license_owner_pubkey,
                &self.provider_pubkey,
                bk_node_owner_pubkey,
                bk_bls_pubkey,
            );
            let signature = STANDARD.decode(&self.delegation_confirm_sig)?;
            let pub_key = hex::decode(&self.provider_pubkey)?;
            let pub_key: &[u8; 32] = pub_key.as_slice().try_into()?;

            ed25519_verify(pub_key, &data, &signature)
                .map_err(|_| IgniterError::DelegationConfirmSig)?;
            Ok(())
        };
        check().map_err(|_| IgniterError::DelegationConfirmSig)
    }

    pub fn check_signatures(
        &self,
        backend_pk: &[u8; 32],
        bk_node_owner_pubkey: &str,
        bk_bls_pubkey: &str,
    ) -> Result<(), IgniterError> {
        self.check_license_proof_sig(backend_pk)?;
        self.check_delegation_sig()?;
        self.check_delegation_confirm_sig(bk_node_owner_pubkey, bk_bls_pubkey)?;
        Ok(())
    }

    pub fn check_all_signatures_in_section(
        signatures: &Vec<LicenceSignature>,
        backend_pk: &str,
        bk_node_owner_pubkey: &str,
        bk_bls_pubkey: &str,
    ) -> Result<(), IgniterError> {
        if signatures.len() > MAX_ALLOWED_LICENSES as usize {
            return Err(IgniterError::TooManyLicenses);
        }
        if signatures.is_empty() {
            return Err(IgniterError::NoLicenses);
        }
        let backend_pubkey_bytes =
            hex::decode(backend_pk).map_err(|_| IgniterError::InvalidBackedKey)?;

        let backend_pubkey_slice: &[u8; 32] = backend_pubkey_bytes
            .as_slice()
            .try_into()
            .map_err(|_| IgniterError::InvalidBackedKey)?;

        // Check all signatures and check that all are unique
        let mut seen = HashSet::new();
        for sig in signatures {
            sig.check_signatures(backend_pubkey_slice, bk_node_owner_pubkey, bk_bls_pubkey)?;
            if !seen.insert(sig.license_id.clone()) {
                return Err(IgniterError::DuplicateLicenseId(sig.license_id.to_string()));
            }
        }
        Ok(())
    }

    // Generate `licenses` from `signatures`
    pub fn derive_licences(signatures: &[LicenceSignature]) -> HashMap<String, i32> {
        signatures.iter().fold(HashMap::new(), |mut acc, sig| {
            *acc.entry(sig.license_owner_pubkey.to_string()).or_insert(0) += 1;
            acc
        })
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use serde_json::json;
    use tvm_types::ed25519_sign_with_secret;

    use super::*;
    use crate::config::BlsConfig;
    use crate::config::Config;
    use crate::config::Keys;
    use crate::config::WalletConfig;

    fn reveal_keypair(signing_key: &SigningKey) -> (String, String) {
        let secret_verifying_key_pair = hex::encode(signing_key.to_keypair_bytes());
        let secret_key = &secret_verifying_key_pair[0..64];
        let verifying_key = &secret_verifying_key_pair[64..128];
        (secret_key.into(), verifying_key.into())
    }

    fn create_license_signature(
        backend_signing_key: &[u8],
        amount: u8,
        pubkey: &str,
        bls_pubkey: &str,
    ) -> Vec<LicenceSignature> {
        const TIMESTAMP: u64 = 1234567890;

        let mut csprng = OsRng;
        let owner_signing_key: SigningKey = SigningKey::generate(&mut csprng);
        let owner_verifying_key = reveal_keypair(&owner_signing_key).1;

        let mut csprng = OsRng;
        let provider_signing_key: SigningKey = SigningKey::generate(&mut csprng);
        let provider_verifying_key = reveal_keypair(&provider_signing_key).1;

        let mut lics = vec![];
        for i in 0..amount {
            // Create license_proof_sig
            let license_id = &format!("license_id_{}", i);
            let message = LicenceSignature::license_proof_prepare(license_id, &owner_verifying_key);
            let license_proof_sig =
                STANDARD.encode(ed25519_sign_with_secret(backend_signing_key, &message).unwrap());

            // Create delegation_sig
            let message = LicenceSignature::delegation_prepare(
                license_id,
                &owner_verifying_key,
                &provider_verifying_key,
                TIMESTAMP,
            );
            let delegation_sig = STANDARD
                .encode(ed25519_sign_with_secret(owner_signing_key.as_bytes(), &message).unwrap());

            // Create delegation_confirm_sig
            let message = LicenceSignature::delegation_confirm_prepare(
                license_id,
                &owner_verifying_key,
                &provider_verifying_key,
                pubkey,
                bls_pubkey,
            );
            let delegation_confirm_sig = STANDARD.encode(
                ed25519_sign_with_secret(provider_signing_key.as_bytes(), &message).unwrap(),
            );

            lics.push(LicenceSignature {
                license_id: license_id.to_string(),
                license_owner_pubkey: owner_verifying_key.clone(),
                provider_pubkey: provider_verifying_key.clone(),
                delegation_sig: delegation_sig.to_string(),
                delegation_confirm_sig: delegation_confirm_sig.to_string(),
                timestamp: TIMESTAMP,
                license_proof_sig: license_proof_sig.to_string(),
            })
        }
        lics
    }

    #[test]
    fn test_check_license_proof_sig_failed() {
        let mut csprng = OsRng;
        let backend_signing_key: SigningKey = SigningKey::generate(&mut csprng);
        let pubkey = "3ef72c59a33ba75a484cfb126bd9e55db267cbd944110374d0b78a9e474c6c87";
        let bls_pubkey="8cf7d141cade81a44c8bc58a02b0448e85e77d47d9c644adfe3512d3c5fcdc2a028cfb96aff704a70f2cce27c96cd706";
        let lic_sig =
            &create_license_signature(&backend_signing_key.to_bytes(), 1, pubkey, bls_pubkey)[0];

        let another_backend_signing_key: SigningKey = SigningKey::generate(&mut csprng);
        let verifying_key = &another_backend_signing_key.verifying_key().to_bytes();

        assert!(lic_sig.check_signatures(verifying_key, pubkey, bls_pubkey).is_err());
    }

    #[test]
    fn test_check_license_proof_sig() {
        let mut csprng = OsRng;
        let backend_signing_key: SigningKey = SigningKey::generate(&mut csprng);
        let pubkey = "3ef72c59a33ba75a484cfb126bd9e55db267cbd944110374d0b78a9e474c6c87";
        let bls_pubkey="8cf7d141cade81a44c8bc58a02b0448e85e77d47d9c644adfe3512d3c5fcdc2a028cfb96aff704a70f2cce27c96cd706";
        let lic_sig =
            &create_license_signature(&backend_signing_key.to_bytes(), 1, pubkey, bls_pubkey)[0];
        let verifying_key = &backend_signing_key.verifying_key().to_bytes();

        assert!(lic_sig.check_signatures(verifying_key, pubkey, bls_pubkey).is_ok());
    }

    #[test]
    fn test_create_licences() {
        let signatures = [
            LicenceSignature {
                license_id: "license_id".to_string(),
                license_owner_pubkey: "owner_pubkey_1".to_string(),
                provider_pubkey: "provider_pubkey".to_string(),
                delegation_sig: "delegation_sig".to_string(),
                delegation_confirm_sig: "delegation_confirm_sig".to_string(),
                timestamp: 123,
                license_proof_sig: "license_proof_sig".to_string(),
            },
            LicenceSignature {
                license_id: "license_id".to_string(),
                license_owner_pubkey: "owner_pubkey_2".to_string(),
                provider_pubkey: "provider_pubkey".to_string(),
                delegation_sig: "delegation_sig".to_string(),
                delegation_confirm_sig: "delegation_confirm_sig".to_string(),
                timestamp: 123,
                license_proof_sig: "license_proof_sig".to_string(),
            },
            LicenceSignature {
                license_id: "license_id".to_string(),
                license_owner_pubkey: "owner_pubkey_1".to_string(),
                provider_pubkey: "provider_pubkey".to_string(),
                delegation_sig: "delegation_sig".to_string(),
                delegation_confirm_sig: "delegation_confirm_sig".to_string(),
                timestamp: 123,
                license_proof_sig: "license_proof_sig".to_string(),
            },
        ];
        let licences = LicenceSignature::derive_licences(&signatures);
        assert_eq!(licences.get("owner_pubkey_1").unwrap(), &2);
        assert_eq!(licences.get("owner_pubkey_2").unwrap(), &1);
    }

    #[test]
    fn test_generate_licences() {
        let backend_secret = "36288d4924164be5e32bc502c41048e7d0ae3f175470694f5f783386f82e5593";
        let backend_verifying = "f3d50b12650a49d9a5de34a4022843efc9fc9ba120a038f04d50db310f78f147";

        let pubkey = "3ef72c59a33ba75a484cfb126bd9e55db267cbd944110374d0b78a9e474c6c87";
        let bls_pubkey="8cf7d141cade81a44c8bc58a02b0448e85e77d47d9c644adfe3512d3c5fcdc2a028cfb96aff704a70f2cce27c96cd706";

        let mut secret_bytes = hex::decode(backend_secret.as_bytes()).unwrap();
        let verifying_bytes = hex::decode(backend_verifying.as_bytes()).unwrap();

        secret_bytes.extend_from_slice(&verifying_bytes);

        let backend_signing_key = SigningKey::from_keypair_bytes(
            secret_bytes.as_slice().try_into().expect("Vec must have exactly 64 elements"),
        )
        .unwrap();

        let signature =
            create_license_signature(backend_signing_key.as_bytes(), 2, pubkey, bls_pubkey);
        let yaml_string = serde_yaml::to_string(&signature).expect("Failed to serialize");
        println!("Signature:\n{}", yaml_string);
    }

    fn default_config_and_keys() -> (Config, Keys) {
        (
            Config {
                proxies: vec![],
                listen_addr: "127.0.0.1:10000".parse().expect("Invalid SocketAddr format"),
                api_addr: "127.0.0.1:10000".parse().expect("Invalid SocketAddr format"),
                advertise_addr: "127.0.0.1:10000".parse().expect("Invalid SocketAddr format"),
                seeds: vec![],
                node_id: None,
                interval: 5,
                signatures: vec![create_test_signature()],
                auto_update: false,
            },
            Keys {
                wallet: WalletConfig { pubkey: "3ef72c59a33ba75a484cfb126bd9e55db267cbd944110374d0b78a9e474c6c87".to_string(), secret: "def".to_string() },
                bls: BlsConfig {
                    pubkey: "8cf7d141cade81a44c8bc58a02b0448e85e77d47d9c644adfe3512d3c5fcdc2a028cfb96aff704a70f2cce27c96cd706".to_string(),
                    secret: "def".to_string(),
                    rnd: "abc".to_string(),
                },
            },
        )
    }
    fn create_test_signature() -> LicenceSignature {
        LicenceSignature {
            license_id: "5e0d534d-98fd-4024-87b8-8c45414f6e9a".to_string(),
            license_owner_pubkey: "37d545d8725f290b1dcff6e06ad7649a6264249a3202354330bb47da90c7b41f"    .to_string(),
            provider_pubkey: "8e962b104119b17ab09e9aa91ff17e5816f65bb66daa6c14b8ca130f4f0bfcc0" .to_string(),
            license_proof_sig: "c6F8qZ52LNeLLKdrVll5F1/U9eGGPUzJMZw7JcKWSzbO/DmmNXkWlDW+k3GwD1giLMxUPbjPzegqYoLoKOThAg==".to_string(),
            delegation_sig: "00lhq1wiiCs10ISYz2AkrZX9M0TU4YaNba3wG7oCeEFUP6uOav5kGesqntIQ+AKL5Y3nkVw+redFxQbOEuM1Dg=="  .to_string(),
            delegation_confirm_sig: "QCd1iMgEUOd7unQ2Qi1v8EyLNIwphFc2hct+/cAsdAT7VUxUNlNmhbo6SKNHyvX5OKnrpIBf2d5JgQNZj3ueDw==".to_string(),
            timestamp: 1744375960
        }
    }

    #[test]
    fn test_to_gossip_kv_no_signatures() {
        std::env::set_var("DEV_MODE", "true");
        let (config, keys) = default_config_and_keys();
        let params = Params { config, keys, docker_socket: None, docker_config: None };
        let result = params.to_gossip().unwrap();
        assert_eq!(result.len(), 6);
    }
    #[test]
    fn test_to_gossip_kv_one_signature() {
        std::env::set_var("DEV_MODE", "true");
        let (mut config, keys) = default_config_and_keys();
        config.signatures = vec![create_test_signature()];
        let params = Params { config, keys, docker_socket: None, docker_config: None };
        let result = params.to_gossip().unwrap();
        assert_eq!(result.len(), 6);
        let hashmap: HashMap<String, String> = result.into_iter().collect();

        assert_eq!(
            hashmap[&ZerostateKeys::Licenses.to_string()],
            json!({"37d545d8725f290b1dcff6e06ad7649a6264249a3202354330bb47da90c7b41f": 1})
                .to_string()
        );
    }

    #[test]
    #[ignore = "this test was written for different BK_SIGNING_KEY=f3d50b12650a49d9a5de34a4022843efc9fc9ba120a038f04d50db310f78f147"]
    fn test_to_gossip_two_signatures_with_dedup() {
        let (mut config, keys) = default_config_and_keys();
        let signature_0 = create_test_signature();
        let signature_1 = LicenceSignature {
            license_id: "license_id_1".to_string(),
            license_owner_pubkey: "46e0aa2a12cdbcb30406a8719d912d80dfae16f3724d3d687d932f221a1010af".to_string(),
            provider_pubkey: "aef18f1ca3ff7a71d153c0571eeeaa8c80eeb1b6c41b25436cec841365f51e2f".to_string(),
            delegation_sig: "Ez4V1gE10bVp/BXc1A7UrXiGiznTbJ1lSue6QtIcDSoyOtHtLYtrXKx89XjHH01XTbEaQ12zbYDvgTHysCeCDw==".to_string(),
            delegation_confirm_sig: "KDNDF8pH/0UK7e7960A2JasWfSmVjFyARulIh9Nj93Cvlzf3VNuZO4qDCQJonl5yQ817Z9hKLWss5AKR6tICDQ==".to_string(),
            timestamp: 1234567890,
            license_proof_sig: "7KuPIWr4QXhmK0eBRQfJuowna/QeA7utUalQofKBrgJKUfve1fwIvXjHsNgqvuBTiFyKudlcwcCTP6N4njgdDw==".to_string(),
        };
        // the same signature twice
        config.signatures = vec![signature_0.clone(), signature_1.clone(), signature_0.clone()];
        let params = Params { config, keys, docker_socket: None, docker_config: None };
        let result = params.to_gossip().unwrap();
        assert_eq!(result.len(), 7);

        let hashmap: HashMap<String, String> = result.into_iter().collect();
        assert_eq!(
            hashmap[&ZerostateKeys::Licenses.to_string()],
            json!({"46e0aa2a12cdbcb30406a8719d912d80dfae16f3724d3d687d932f221a1010af": 2})
                .to_string()
        );
    }

    #[test]
    fn test_back_and_front_use_the_same_algorithm() {
        std::env::set_var("DEV_MODE", "true");
        let lic = LicenceSignature {
            license_id: "2aebf602-7503-4572-976c-79f206f9b2c0".to_string(),
            license_proof_sig: "4tvVKDRZPKOkV+bqTjUSEuNPP4zYio7kodo+UylCzvFCKEYUhGjF4VF5JbGzU/s2l98V31lMvBHKPv1yvw6dDg==".to_string(),
            license_owner_pubkey: "7876682d123554aeedc71eb4e437e3c25ea8c9d97c0fd3fb9521061d6f494cdc".to_string(),
            provider_pubkey: "b8727272b106cd6b0712d18a747432577256e0a14f73e5a187a2f98e175034fc".to_string(),
            delegation_sig: "qwX6siO6q5jd7JFlcsc31maYdcL/XHgOuXqdS9UW9FPydvqmafMR78BvFrnJ6/7aT98ChLkaPuFf+PpYQczbCA==".to_string(),
            delegation_confirm_sig: "delegation_confirm_sig".to_string(),
            timestamp: 1736944335,
        };
        let backend_verifying_key =
            hex::decode(BACKEND_VERIFYING_KEY.as_bytes()).expect("Can't decode hex");

        let backend_verifying_key: &[u8; 32] =
            backend_verifying_key.as_slice().try_into().expect("Vec must have exactly 32 elements");

        assert!(lic.check_license_proof_sig(backend_verifying_key).is_ok());
        assert!(lic.check_delegation_sig().is_ok());
    }
}
