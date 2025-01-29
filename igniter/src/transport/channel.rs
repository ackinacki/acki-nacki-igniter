use std::net::SocketAddr;
use std::sync::atomic;
use std::sync::atomic::Ordering;

use anyhow::Context;
use async_channel::Receiver;
use async_channel::Sender;
use async_trait::async_trait;
use chitchat::transport::Socket;
use chitchat::transport::Transport;
use chitchat::ChitchatMessage;
use chitchat::Serializable;
use tracing::debug;

use crate::cli::CLI;

/// Universal channel transport fasade.
/// Allows using in tokio/multithread context.
#[derive(Debug)]
pub struct ChannelTransport {
    counter: atomic::AtomicU16,
    incoming_source: Receiver<(SocketAddr, ChitchatMessage)>,
    outgoing_source: Sender<(SocketAddr, ChitchatMessage)>,
}

impl ChannelTransport {
    pub fn new(
        incoming_source: Receiver<(SocketAddr, ChitchatMessage)>,
        outgoing_source: Sender<(SocketAddr, ChitchatMessage)>,
    ) -> Self {
        let counter = atomic::AtomicU16::new(0);
        Self { counter, incoming_source, outgoing_source }
    }
}

#[async_trait]
impl Transport for ChannelTransport {
    async fn open(&self, _listen_addr: SocketAddr) -> anyhow::Result<Box<dyn Socket>> {
        self.counter.fetch_add(1, Ordering::SeqCst);
        let id = self.counter.load(Ordering::SeqCst);
        tracing::info!("transport open {} {:?}", id, &CLI.config.node_id);
        let incoming_channel = self.incoming_source.clone();
        let outgoing_channel = self.outgoing_source.clone();
        Ok(Box::new(ChannelSocket { id, incoming_channel, outgoing_channel }))
    }
}

#[derive(Debug)]
struct ChannelSocket {
    id: u16,
    incoming_channel: Receiver<(SocketAddr, ChitchatMessage)>,
    outgoing_channel: Sender<(SocketAddr, ChitchatMessage)>,
}

#[async_trait]
impl Socket for ChannelSocket {
    async fn send(&mut self, to_addr: SocketAddr, message: ChitchatMessage) -> anyhow::Result<()> {
        debug!("send message to {to_addr}");
        self.outgoing_channel.send((to_addr, message)).await?;
        Ok(())
    }

    async fn recv(&mut self) -> anyhow::Result<(SocketAddr, ChitchatMessage)> {
        tracing::info!("chitchat call recv {}", self.id);
        let (from_addr, message) = self
            .incoming_channel
            .recv()
            .await
            .inspect_err(|err| tracing::error!(%err, "channel recv failed"))
            .context("channel closed")?;
        tracing::info!("chitchat recv message from {from_addr} {}", message.serialized_len());
        Ok((from_addr, message))
    }
}
