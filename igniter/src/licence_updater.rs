use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::bail;
use anyhow::Context;
use chitchat::Chitchat;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::time::interval;
use url::Url;

use crate::Keys;
use crate::ZerostateKeys;

pub const MAX_ALLOWED_LICENSES: u32 = 5;
const MAINNET_LIC_UPDATE_URL: &str = "https://dashboard.ackinacki.com/api/gossip/licenses";
const ERR_NO_LICENSES: &str = "There are no licenses associated with the provided public key";
const ERR_TOO_MANY_LICENSES: &str =
    "The provided public key has too many licenses associated with it";

const LIC_UPDATE_SECS: u64 = 60;

pub struct LicenceUpdater {
    url: Url,
    client: Client,
    interval: Duration,
    chitchat: Arc<Mutex<chitchat::Chitchat>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct LicensesResponse(HashMap<String, u32>);

impl LicensesResponse {
    fn total_licenses(&self) -> u32 {
        self.0.values().sum()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn inner(&self) -> &HashMap<String, u32> {
        &self.0
    }

    fn validate(&self) -> anyhow::Result<()> {
        if self.is_empty() {
            bail!(ERR_NO_LICENSES);
        }
        if self.total_licenses() > MAX_ALLOWED_LICENSES {
            bail!(ERR_TOO_MANY_LICENSES,);
        }
        Ok(())
    }
}

pub async fn run_licence_updater(
    gossip: Arc<Mutex<Chitchat>>,
    keys: Keys,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let url = build_url(&keys.wallet.pubkey).context("Failed to build license update URL")?;
    Ok(tokio::spawn(async move {
        LicenceUpdater::new(url, Duration::from_secs(LIC_UPDATE_SECS), gossip).run().await
    }))
}

pub fn build_url(node_pubkey: &str) -> anyhow::Result<Url> {
    let endpoint =
        std::env::var("LIC_UPDATE_URL").unwrap_or_else(|_| MAINNET_LIC_UPDATE_URL.to_string());

    let mut url = Url::parse(&endpoint).context("Invalid license update URL")?;

    url.set_query(Some(&format!("node_pubkey={node_pubkey}")));

    Ok(url)
}

impl LicenceUpdater {
    pub fn new(url: Url, interval: Duration, chitchat: Arc<Mutex<chitchat::Chitchat>>) -> Self {
        Self { url, client: Client::new(), interval, chitchat }
    }

    pub async fn run(&mut self) {
        let mut interval = interval(self.interval);

        loop {
            interval.tick().await;
            match self.query().await {
                Ok(license_info) => {
                    let mut guard = self.chitchat.lock().await;
                    guard.self_node_state().set(ZerostateKeys::Licenses, license_info);
                }
                Err(err) => tracing::warn!("Failed to update license info: {:?}", err),
            }
        }
    }

    async fn query(&self) -> anyhow::Result<String> {
        tracing::debug!("Quering {:?}", self.url.clone());
        let response = self.client.get(self.url.clone()).send().await?;
        tracing::trace!("Response {:?}", response);

        let body = response.text().await?;
        tracing::trace!("Got text {:?}", body);

        let _ = check_license_number(&body)?;
        Ok(body)
    }
}

pub fn check_license_number(body: &str) -> anyhow::Result<LicensesResponse> {
    let licenses: LicensesResponse =
        serde_json::from_str(body).context("Failed to parse license response")?;

    licenses.validate()?;
    Ok(licenses)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_check_license_number() {
        let body = "{\"pubkey1\":1, \"pubkey2\":2}".to_string();

        let mut hm = HashMap::new();
        hm.insert("pubkey1".to_string(), 1);
        hm.insert("pubkey2".to_string(), 2);

        assert_eq!(check_license_number(&body).unwrap(), LicensesResponse(hm));
    }

    #[test]
    fn test_too_many_licenses() {
        let body = "{\"pubkey1\":4, \"pubkey2\":2}".to_string();
        let result = check_license_number(&body);
        if let Err(error) = result {
            assert_eq!(format!("{}", error), ERR_TOO_MANY_LICENSES);
        } else {
            panic!("License check should not pass!")
        }
    }

    #[test]
    fn test_no_licenses() {
        let body = "{}".to_string();
        let result = check_license_number(&body);
        if let Err(error) = result {
            assert_eq!(format!("{}", error), ERR_NO_LICENSES);
        } else {
            panic!("License check should not pass!")
        }
    }
}
