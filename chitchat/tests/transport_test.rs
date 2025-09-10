use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use chitchat::spawn_chitchat;
use chitchat::ChitchatConfig;
use chitchat::ChitchatId;
use chitchat::FailureDetectorConfig;
use itertools::Itertools;
use once_cell::sync::OnceCell;
use rustls_pki_types::pem::PemObject;
use tracing_subscriber::EnvFilter;

static LOG_INIT: OnceCell<()> = OnceCell::new();

fn init_logs() {
    LOG_INIT.get_or_init(|| {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_test_writer()
            .init();
    });
}
// #[derive(Clone)]
// struct ConstDelay(f32);
//
// impl DelayMillisDist for ConstDelay {}
//
// impl Distribution<f32> for ConstDelay {
//     fn sample<R: Rng + ?Sized>(&self, _rng: &mut R) -> f32 {
//         self.0
//     }
// }

trait TransportFactory {
    fn create_transport(&self, credential: NetCredential) -> Box<dyn Transport>;
}

struct Udp;

impl TransportFactory for Udp {
    fn create_transport(&self, _credential: NetCredential) -> Box<dyn Transport> {
        Box::new(UdpTransport)
    }
}

struct Channel(ChannelTransport);
impl Channel {
    fn new() -> Self {
        Self(ChannelTransport::with_mtu(100_000))
    }
}

impl TransportFactory for Channel {
    fn create_transport(&self, _credential: NetCredential) -> Box<dyn Transport> {
        Box::new(self.0.clone())
    }
}

struct Quic<T: NetTransport + Clone>(T);
impl<T: NetTransport + 'static> TransportFactory for Quic<T> {
    fn create_transport(&self, credential: NetCredential) -> Box<dyn Transport> {
        Box::new(TransportLayerTransport::new(self.0.clone(), credential))
    }
}

#[tokio::test]
#[ignore]
async fn test_quic_transport() {
    test_transport(Quic(MsQuicTransport::new())).await;
}

#[tokio::test]
#[ignore]
async fn test_udp_transport() {
    test_transport(Udp).await;
}

#[tokio::test]
#[ignore]
async fn test_inproc_transport() {
    test_transport(Channel::new()).await;
}

const BASE_PORT: u16 = 11000;

use chitchat::transport::ChannelTransport;
use chitchat::transport::Transport;
use chitchat::transport::TransportLayerTransport;
use chitchat::transport::UdpTransport;
use transport_layer::msquic::MsQuicTransport;
use transport_layer::NetCredential;
use transport_layer::NetTransport;

async fn test_transport(transport_factory: impl TransportFactory) {
    init_logs();
    tracing::trace!("test_gossip_over_quic");
    let host_count = 500;
    let key_value_count = 10;
    // let seed_hosts = [0, 1, 2, 3, 4];
    let seed_hosts = 0..host_count;

    let credential_key = include_bytes!("key.pem");
    let credential_cert = include_bytes!("cert.pem");
    let credential_key = rustls_pki_types::PrivateKeyDer::from_pem_slice(credential_key).unwrap();
    let credential_cert =
        rustls_pki_types::CertificateDer::from_pem_slice(credential_cert).unwrap();

    let mut addrs = Vec::new();
    let mut credentials = Vec::new();
    let mut seed_nodes = Vec::new();
    for host in 0..host_count {
        let addr = SocketAddr::from(([127, 0, 0, 1], BASE_PORT + host as u16));
        addrs.push(addr);
        credentials.push(NetCredential {
            my_key: credential_key.clone_key(),
            my_certs: vec![credential_cert.clone()],
            root_certs: vec![],
        });
        if seed_hosts.contains(&host) {
            seed_nodes.push(addr.to_string());
        }
    }
    let mut handles = Vec::new();
    for host in 0..host_count {
        let listen_addr = addrs[host];
        let transport = transport_factory.create_transport(credentials[host].clone());
        let chitchat_id = ChitchatId {
            node_id: format!("node_{host}"),
            generation_id: 0,
            gossip_advertise_addr: listen_addr,
        };
        let config = ChitchatConfig {
            chitchat_id,
            cluster_id: "default-cluster".to_string(),
            gossip_interval: Duration::from_millis(500),
            listen_addr,
            seed_nodes: seed_nodes.clone(),
            failure_detector_config: FailureDetectorConfig::default(),
            marked_for_deletion_grace_period: Duration::from_secs(1000),
            catchup_callback: None,
            extra_liveness_predicate: None,
        };
        let handle = spawn_chitchat(config, Vec::new(), transport.as_ref()).await.unwrap();
        handle
            .with_chitchat(|x| {
                for i in 0..key_value_count {
                    x.self_node_state().set(format!("key{i}"), i.to_string().repeat(100));
                }
            })
            .await;
        handles.push(handle);
    }
    for sec in 0..60 {
        println!("\n=== {sec} ===\n");
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut dead_ids = HashMap::new();
        let mut live_ids = HashMap::new();
        for handle in &handles {
            let chitchat = handle.chitchat();
            let chitchat = chitchat.lock();
            let live = chitchat.live_nodes().collect::<Vec<_>>();
            let dead = chitchat.dead_nodes().collect::<Vec<_>>();
            insert_ids(&mut dead_ids, &dead);
            insert_ids(&mut live_ids, &live);
        }
        live_ids.retain(|_, c| *c < host_count);
        print_ids("live", &live_ids);
        // print_ids("dead", &dead_ids);
    }
}

fn insert_ids(ids: &mut HashMap<usize, usize>, new_ids: &[&ChitchatId]) {
    for id in new_ids {
        let host = (id.gossip_advertise_addr.port() - BASE_PORT) as usize;
        ids.entry(host).and_modify(|x| *x += 1).or_insert(1);
    }
}

fn print_ids(prefix: &str, ids: &HashMap<usize, usize>) {
    println!("{prefix}: {}, {}", ids.len(), ids.values().map(|c| format!("{c}")).join(","));
}
