use std::net::SocketAddr;
use std::net::UdpSocket;
use std::time::Duration;

use anyhow::Context;
use async_channel::Sender;
use chitchat::ChitchatMessage;
use chitchat::Deserializable;
use tokio::io::AsyncReadExt;
use tokio::task::JoinSet;
use tracing::info;
use wtransport::endpoint::IncomingSession;

pub async fn run(
    socket: UdpSocket,
    incoming_messages: Sender<(SocketAddr, ChitchatMessage)>,
) -> anyhow::Result<()> {
    let identity = wtransport::Identity::self_signed(["localhost"])?;
    let server_config = wtransport::ServerConfig::builder()
        .with_bind_socket(socket)
        // .with_bind_address(bind_addr)
        .with_identity(identity)
        .keep_alive_interval(Some(Duration::from_millis(400)))
        .max_idle_timeout(Some(Duration::from_secs(1)))?
        .build();

    let server_endpoint = wtransport::Endpoint::server(server_config)
        .with_context(|| "failed to build quic server")?;

    let mut join_set: JoinSet<anyhow::Result<()>> = JoinSet::new();

    let (incoming_session_s, mut incoming_session_r) = tokio::sync::mpsc::channel(1);

    join_set.spawn(async move {
        loop {
            let incoming_session = server_endpoint.accept().await;
            tracing::warn!("QUIC incoming session");
            incoming_session_s
                .send(incoming_session)
                .await
                .expect("QUIC incoming session channel sender closed");
        }
    });

    loop {
        tokio::select! {
            v = join_set.join_next() => {
                match v {
                    None => {
                        anyhow::bail!("critical: join set is empty");
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        // TODO: improve error messages
                        tracing::warn!(%err, "connection lost");
                    }
                }
            }
            v = incoming_session_r.recv() => {
                let incoming_session = v.expect("QUIC incoming session channel receiver closed");
                let incoming_messages = incoming_messages.clone();
                // tokio::spawn(async move {
                //     handle_incoming_session(incoming_session, incoming_messages).await?;
                //     Ok(())
                // });
                join_set.spawn(async move {
                    handle_incoming_session(incoming_session, incoming_messages).await?;
                    Ok(())
                });
            }
        }
    }
}

async fn handle_incoming_session(
    incoming_session: IncomingSession,
    incoming_messages: Sender<(SocketAddr, ChitchatMessage)>,
) -> anyhow::Result<()> {
    let start = std::time::Instant::now();

    let session_request = incoming_session.await?;
    let from_addr = session_request.remote_address();
    info!("new connection from {from_addr}");
    let connection = session_request.accept().await?;

    let mut uni_stream = connection.accept_uni().await?;
    let mut uni_buf = Vec::new();
    uni_stream.read_to_end(&mut uni_buf).await?;

    let len = uni_buf.len();

    let buf: &mut &[u8] = &mut uni_buf.as_slice();
    let real_from_addr = SocketAddr::deserialize(buf)?;
    tracing::warn!("{from_addr:>20} {real_from_addr:>20}: got {len} bytes");
    let message = ChitchatMessage::deserialize(buf)?;

    info!(
        "sending for processing {:?} sender_count {} sender_len {}",
        start.elapsed(),
        incoming_messages.sender_count(),
        incoming_messages.len(),
    );

    incoming_messages.send((real_from_addr, message)).await?;
    info!("sent for processing {:?}", start.elapsed());

    Ok(())
}
