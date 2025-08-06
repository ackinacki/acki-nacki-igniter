use chitchat::ChitchatRef;
use tokio::task::JoinHandle;

use crate::open_api::routes::extract_verified_state_without_licences;

// This watcher print warn message every minute if some of delegated licenses was revoked
pub async fn run(chitchat: ChitchatRef, pubkey: String) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let node_states = chitchat.lock().state_snapshot().node_states;
            let (_, revoked_licenses) = extract_verified_state_without_licences(node_states);
            for license in revoked_licenses.into_iter() {
                if license.provider_pubkey == pubkey {
                    eprintln!("WARNING: Licence with id {} was revoked", license.license_id)
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }
    })
}
