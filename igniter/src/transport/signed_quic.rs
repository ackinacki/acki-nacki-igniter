use core::panic;
use std::collections::HashSet;
use std::net::SocketAddr;

use anyhow::Context;
use async_trait::async_trait;
use chitchat::transport::Socket;
use chitchat::transport::Transport;
use chitchat::ChitchatMessage;
use chitchat::Deserializable;
use chitchat::Serializable;
use ed25519_dalek::ed25519::signature::SignerMut;
use ed25519_dalek::Signature;
use ed25519_dalek::SigningKey;
use ed25519_dalek::Verifier;
use ed25519_dalek::VerifyingKey;
use tokio::io::AsyncReadExt;
use tokio::sync::oneshot;
use tracing::error;
use tracing::info;
use tracing::warn;
use wtransport::endpoint::endpoint_side::Client;
use wtransport::endpoint::endpoint_side::Server;
use wtransport::endpoint::IncomingSession;
use wtransport::endpoint::SessionRequest;
use wtransport::Endpoint;
use wtransport::Identity;
use wtransport::ServerConfig;

/// Maximum UDP datagram payload size (in bytes).
///
/// Note that 65KB typically won't fit in a single IP packet,
/// so long messages will be sent over several IP fragments of MTU size.
///
/// We pick a large payload size because at the moment because
/// we send the self digest "in full".
/// An Ethernet frame size of 1400B would limit us to 20 nodes
/// or so.
pub const MAX_UDP_DATAGRAM_PAYLOAD_SIZE: usize = 65_507;
// pub const MAX_UDP_DATAGRAM_PAYLOAD_SIZE: usize = 1_400;

pub struct QuicTransport {
    pub pubkeys: Vec<VerifyingKey>,
    pub signing_key: SigningKey,
    quic_handler_tx: oneshot::Sender<tokio::task::JoinHandle<()>>,
}

impl QuicTransport {
    pub fn new(
        pubkeys: Vec<VerifyingKey>,
        signing_key: SigningKey,
        quic_handler_tx: oneshot::Sender<tokio::task::JoinHandle<()>>,
    ) -> QuicTransport {
        QuicTransport { pubkeys, signing_key, quic_handler_tx }
    }

    pub fn send_handler(self, handle: tokio::task::JoinHandle<()>) {
        if self.quic_handler_tx.is_closed() {
            error!("inject_tx is closed");
            panic!("inject_tx is closed");
        }
        self.quic_handler_tx.send(handle).unwrap();
    }
}

pub async fn init_quic_server(bind_addr: SocketAddr) -> anyhow::Result<()> {
    let identity = wtransport::Identity::self_signed(["localhost"])?;
    let server_config = wtransport::ServerConfig::builder()
        .with_bind_address(bind_addr)
        .with_identity(identity)
        .build();
    let endpoint = wtransport::Endpoint::server(server_config)
        .with_context(|| "failed to build quic server")?;

    for i in 0.. {
        let incoming = endpoint.accept().await;

        tokio::spawn(async move {
            let session_request = incoming.await.unwrap();
            let from_addr = session_request.remote_address();
            let connection = session_request.accept().await.unwrap();

            let mut uni_stream = connection.accept_uni().await.unwrap();
            let mut uni_buf = Vec::new();
            uni_stream.read_to_end(&mut uni_buf).await.unwrap();
            eprintln!("{i:>10}: got {} bytes from {from_addr}", uni_buf.len());
        });
    }

    Ok(())
}
pub async fn quic_client() {}

#[async_trait]
impl Transport for QuicTransport {
    async fn open(&self, bind_addr: SocketAddr) -> anyhow::Result<Box<dyn Socket>> {
        tokio::spawn(async move {
            tokio::select! {
                _ = quic_server() => {}
                _ = quic_client() => {}
            }
            panic!("quic handler exited");
        });

        let udp_socket =
            QuicSocket::open(bind_addr, self.pubkeys.clone(), self.signing_key.clone()).await?;
        Ok(Box::new(udp_socket))
    }
}

pub struct QuicSocket {
    buf_send: Vec<u8>,
    buf_recv: Box<[u8; MAX_UDP_DATAGRAM_PAYLOAD_SIZE]>,
    pubkey_set: HashSet<VerifyingKey>,
    signing_key: SigningKey,
    quic_client: Endpoint<Client>,
    quic_server: Endpoint<Server>,
}

impl QuicSocket {
    pub async fn open(
        bind_addr: SocketAddr,
        pubkeys: impl IntoIterator<Item = VerifyingKey>,
        signing_key: SigningKey,
    ) -> anyhow::Result<QuicSocket> {
        let identity = Identity::self_signed(["localhost", "127.0.0.1"])?;
        let udp_socket = std::net::UdpSocket::bind(bind_addr)
            .with_context(|| format!("failed to bind to {bind_addr}/UDP"))?;
        info!(%bind_addr, ?udp_socket, "bound UDP socket");
        let server_config =
            ServerConfig::builder().with_bind_socket(udp_socket).with_identity(identity).build();

        let quic_server = Endpoint::server(server_config)
            .with_context(|| format!("failed to build quic server to {bind_addr}/UDP"))?;

        let client_config = wtransport::ClientConfig::builder()
            .with_bind_default()
            .with_no_cert_validation()
            .build();

        let quic_client = Endpoint::client(client_config)
            .with_context(|| format!("failed to build quic client to {bind_addr}/UDP"))?;

        let pubkey_set = HashSet::from_iter(pubkeys);
        Ok(QuicSocket {
            buf_send: Vec::with_capacity(MAX_UDP_DATAGRAM_PAYLOAD_SIZE),
            buf_recv: Box::new([0u8; MAX_UDP_DATAGRAM_PAYLOAD_SIZE]),
            pubkey_set,
            signing_key,
            quic_client,
            quic_server,
        })
    }
}

pub const PROTOCOL_VERSION: u8 = 0;

#[async_trait]
impl Socket for QuicSocket {
    async fn send(&mut self, to_addr: SocketAddr, message: ChitchatMessage) -> anyhow::Result<()> {
        self.buf_send.clear();

        if message.serialized_len() > MAX_UDP_DATAGRAM_PAYLOAD_SIZE - SIGNED_MESSAGE_HEADER_LENGTH {
            anyhow::bail!("message is too long {:?}", message);
        }

        PROTOCOL_VERSION.serialize(&mut self.buf_send);

        let message_buf = message.serialize_to_vec();

        // sign the message
        let signature = self.signing_key.sign(&message_buf);
        self.buf_send.extend(signature.to_bytes());
        self.buf_send.extend(self.signing_key.verifying_key().as_bytes());

        self.buf_send.extend(&message_buf);

        info!(%to_addr, "sending message");

        self.send_bytes(to_addr, &self.buf_send).await?;
        Ok(())
    }

    /// Recv needs to be cancellable.
    async fn recv(&mut self) -> anyhow::Result<(SocketAddr, ChitchatMessage)> {
        loop {
            match self.receive_verified_one().await {
                Ok(message) => return Ok(message),
                Err(err) => {
                    tracing::warn!(%err, "recv failed");
                    continue;
                }
            }
        }
    }
}

pub const SIGNED_MESSAGE_HEADER_LENGTH: usize = 1 // size_of_val(&PROTOCOL_VERSION)
    + ed25519_dalek::SIGNATURE_LENGTH
    + ed25519_dalek::PUBLIC_KEY_LENGTH;

impl QuicSocket {
    async fn receive_verified_one(&mut self) -> anyhow::Result<(SocketAddr, ChitchatMessage)> {
        let incoming_session: IncomingSession = self.quic_server.accept().await;
        let session_request: SessionRequest = incoming_session.await?;

        let from_addr = session_request.remote_address();
        let connection = session_request.accept().await?;

        let mut stream = connection.accept_uni().await?;

        // TODO: progressive load
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await?;

        let len = buf.len();

        // let (len, from_addr) = self
        //     .socket
        //     .recv_from(&mut self.buf_recv[..])
        //     .await
        //     .context("Error while receiving UDP message")?;

        //
        if len < SIGNED_MESSAGE_HEADER_LENGTH {
            anyhow::bail!("invalid payload len");
        }

        // let (buf, _) = self.buf_recv.split_at(len);

        //
        let (protocol_version, buf) = buf.split_first().context("failed to split buf")?;
        if *protocol_version != PROTOCOL_VERSION {
            anyhow::bail!("invalid protocol version");
        }

        //
        let (signature_buf, buf) = buf.split_first_chunk().context("BUG: failed to split buf")?;
        let (pubkey_buf, mut msg_buf) =
            buf.split_first_chunk().context("BUG: failed to split buf")?;

        // IMPORTANT! check whitelist
        let verifier = VerifyingKey::from_bytes(pubkey_buf)?;
        // if !self.pubkey_set.contains(&verifier) {
        //     anyhow::bail!("verifier not in the whitelist: {:?}", verifier);
        // }

        // IMPORTANT! check signature
        let signature = Signature::from_bytes(signature_buf);
        verifier.verify(msg_buf, &signature).context("Invalid signature")?;

        let message = ChitchatMessage::deserialize(&mut msg_buf).context("Invalid message")?;
        Ok((from_addr, message))
    }

    pub(crate) async fn send_bytes(
        &self,
        to_addr: SocketAddr,
        payload: &[u8],
    ) -> anyhow::Result<()> {
        let url = format!("https://{to_addr}");

        info!(%url, "sending bytes");
        let connection = self.quic_client.connect(url).await?;

        let mut stream = connection.open_uni().await?.await?;

        stream.write_all(payload).await?;
        stream.finish().await?;

        Ok(())
    }
}
