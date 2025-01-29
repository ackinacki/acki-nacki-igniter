pub mod incoming;
pub mod outgoing;

use std::net::SocketAddr;
use std::net::UdpSocket;

use async_channel::Receiver;
use async_channel::Sender;
use chitchat::ChitchatMessage;
use tokio::task::JoinHandle;

pub const CHANNEL_CAPACITY: usize = 100;

pub fn run(
    bind_addr: SocketAddr,
    advertise_addr: SocketAddr,
) -> anyhow::Result<(
    JoinHandle<anyhow::Result<()>>,
    Receiver<(SocketAddr, ChitchatMessage)>,
    Sender<(SocketAddr, ChitchatMessage)>,
)> {
    let (incoming_messages_s, incoming_messages_r) = async_channel::bounded(CHANNEL_CAPACITY);
    let (outgoing_messages_s, outgoing_messages_r) = async_channel::bounded(CHANNEL_CAPACITY);

    let socket = UdpSocket::bind(bind_addr)?;
    let socket_clone = socket.try_clone()?;

    let incoming_handler = tokio::spawn(outgoing::run(socket, advertise_addr, outgoing_messages_r));
    let outgoing_handler = tokio::spawn(incoming::run(socket_clone, incoming_messages_s));

    let service_handler = tokio::spawn(async move {
        tokio::select! {
            v = incoming_handler => {
                anyhow::bail!("incoming handler failed: {v:?}");
            }
            v = outgoing_handler => {
                anyhow::bail!("outgoing handler failed: {v:?}");
            }
        }
    });

    Ok((service_handler, incoming_messages_r, outgoing_messages_s))
}
