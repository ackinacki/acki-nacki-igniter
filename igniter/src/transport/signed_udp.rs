use std::net::SocketAddr;

use anyhow::Context;
use async_trait::async_trait;
use chitchat::transport::Socket;
use chitchat::transport::Transport;
use chitchat::transport::UdpTransport;
use chitchat::ChitchatMessage;
use chitchat::Deserializable;
use chitchat::Serializable;
use ed25519_dalek::ed25519::signature::SignerMut;
use ed25519_dalek::Signature;
use ed25519_dalek::SigningKey;
use ed25519_dalek::Verifier;
use ed25519_dalek::VerifyingKey;

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

pub struct UdpSignedTransport {
    pub pubkeys: Vec<VerifyingKey>,
    pub signing_key: SigningKey,
    pub transport: UdpTransport,
}

impl UdpSignedTransport {
    pub fn new(
        pubkeys: Vec<VerifyingKey>,
        signing_key: SigningKey,
        transport: UdpTransport,
    ) -> UdpSignedTransport {
        UdpSignedTransport { pubkeys, signing_key, transport }
    }
}

#[async_trait]
impl Transport for UdpSignedTransport {
    async fn open(&self, bind_addr: SocketAddr) -> anyhow::Result<Box<dyn Socket>> {
        let udp_socket = UdpSignedSocket::open(bind_addr, self.signing_key.clone()).await?;
        Ok(Box::new(udp_socket))
    }
}

pub struct UdpSignedSocket {
    buf_send: Vec<u8>,
    buf_recv: Box<[u8; MAX_UDP_DATAGRAM_PAYLOAD_SIZE]>,
    socket: tokio::net::UdpSocket,
    signing_key: SigningKey,
}

impl UdpSignedSocket {
    pub async fn open(
        bind_addr: SocketAddr,
        signing_key: SigningKey,
    ) -> anyhow::Result<UdpSignedSocket> {
        let socket = tokio::net::UdpSocket::bind(bind_addr)
            .await
            .with_context(|| format!("failed to bind to {bind_addr}/UDP for gossip"))?;
        Ok(UdpSignedSocket {
            buf_send: Vec::with_capacity(MAX_UDP_DATAGRAM_PAYLOAD_SIZE),
            buf_recv: Box::new([0u8; MAX_UDP_DATAGRAM_PAYLOAD_SIZE]),
            socket,
            signing_key,
        })
    }
}

pub const PROTOCOL_VERSION: u8 = 0;

#[async_trait]
impl Socket for UdpSignedSocket {
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

impl UdpSignedSocket {
    async fn receive_verified_one(&mut self) -> anyhow::Result<(SocketAddr, ChitchatMessage)> {
        let (len, from_addr) = self
            .socket
            .recv_from(&mut self.buf_recv[..])
            .await
            .context("Error while receiving UDP message")?;

        //
        if len < SIGNED_MESSAGE_HEADER_LENGTH {
            anyhow::bail!("invalid payload len");
        }

        let (buf, _) = self.buf_recv.split_at(len);

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
        self.socket
            .send_to(payload, to_addr)
            .await
            .context("failed to send chitchat message to peer")?;
        Ok(())
    }
}
