// 2022-2024 (c) Copyright Contributors to the GOSH DAO. All rights reserved.
//

use std::net::SocketAddr;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::time::Duration;

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use telemetry_utils::mpsc::InstrumentedReceiver;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tvm_types::AccountId;

use crate::msquic::MsQuicNetIncomingRequest;
use crate::msquic::MsQuicTransport;
use crate::NetConnection;
use crate::NetCredential;
use crate::NetIncomingRequest;
use crate::NetListener;
use crate::NetTransport;

const DEFAULT_BROADCAST_CAPACITY: usize = 10;

#[derive(Debug, Clone)]
pub struct LiteServer {
    pub bind: SocketAddr,
}

impl LiteServer {
    pub fn new(bind: SocketAddr) -> Self {
        Self { bind }
    }

    pub async fn start<TBPResolver>(
        self,
        raw_block_receiver: InstrumentedReceiver<(AccountId, Vec<u8>)>,
        bp_resolver: TBPResolver,
    ) -> anyhow::Result<()>
    where
        TBPResolver: Send + Sync + Clone + 'static + FnMut(AccountId) -> Option<String>,
    {
        let (tx, rx) = std::sync::mpsc::channel::<MsQuicNetIncomingRequest>();
        let (btx, _ /* we will subscribe() later */) =
            broadcast::channel(DEFAULT_BROADCAST_CAPACITY);

        let server_handler: JoinHandle<anyhow::Result<()>> = tokio::spawn(async move {
            self.server(tx).await?;
            Ok(())
        });

        let session_handler: JoinHandle<anyhow::Result<()>> = {
            let btx = btx.clone();
            tokio::spawn(async move {
                sessions_handler(rx, btx).await?;
                Ok(())
            })
        };

        let multiplexer_handler: JoinHandle<anyhow::Result<()>> = {
            let btx = btx.clone();
            tokio::spawn(async move {
                message_multiplexor(raw_block_receiver, btx, bp_resolver).await?;
                Ok(())
            })
        };

        tokio::select! {
            v = server_handler => v??,
            v = multiplexer_handler => v??,
            v = session_handler => v??,
        }

        Ok(())
    }

    async fn server(&self, session_sender: Sender<MsQuicNetIncomingRequest>) -> anyhow::Result<()> {
        let transport = MsQuicTransport::new();

        let listener = transport
            .create_listener(self.bind, &["ALPN"], NetCredential::generate_self_signed())
            .await?;

        tracing::info!("LiteServer started on port {}", self.bind.port());

        loop {
            match listener.accept().await {
                Ok(incoming_request) => {
                    tracing::info!("New incoming request");
                    session_sender.send(incoming_request)?;
                }
                Err(error) => tracing::error!("LiteServer can't accept request: {error}"),
            }
        }
    }
}

async fn sessions_handler(
    session_recv: Receiver<MsQuicNetIncomingRequest>,
    btx: broadcast::Sender<Vec<u8>>,
) -> anyhow::Result<()> {
    let logger_handle: JoinHandle<anyhow::Result<()>> = {
        let btx = btx.clone();

        tracing::info!("Prepare Starting broadcaster logger");
        tokio::spawn(async move {
            tracing::info!("Starting broadcaster logger");
            let mut brx = btx.subscribe();
            loop {
                match brx.recv().await {
                    Ok(msg) => {
                        tracing::info!("Received message from broadcast: {:?}", &msg[..10]);
                        tracing::info!("brx len {:?}", brx.len());
                    }
                    Err(err) => {
                        tracing::error!("Error receiving from broadcast: {}", err);
                        anyhow::bail!(err);
                    }
                }
            }
        })
    };

    let mut pool = FuturesUnordered::<JoinHandle<anyhow::Result<()>>>::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<JoinHandle<anyhow::Result<()>>>(20);

    pool.push(logger_handle);
    pool.push(tokio::spawn(async {
        // note: here we guarantie that pool won't stop if no errors accured
        loop {
            tokio::time::sleep(Duration::from_secs(100)).await;
        }
    }));

    pool.push(tokio::spawn(async move {
        loop {
            let incoming_request = session_recv.recv()?;
            let btx = btx.clone();
            tx.send(tokio::spawn(async move {
                let connection = incoming_request.accept().await?;
                let mut brx = btx.subscribe();

                loop {
                    let data = brx.recv().await.map_err(|err| {
                        tracing::error!("brx err: {}", err);
                        err
                    })?;
                    let peer = connection.remote_addr().to_string();
                    match connection.send(&data).await {
                        Ok(_) => {
                            tracing::info!("Sent {} bytes to {peer}", data.len())
                        }
                        Err(err) => {
                            tracing::error!("Can't {} bytes to {peer}: {err}", data.len());
                            anyhow::bail!(err);
                        }
                    }
                }
            }))
            .await?;
        }
    }));

    loop {
        tokio::select! {
            v = rx.recv() => pool.push(v.ok_or_else(|| anyhow::anyhow!("channel was closed"))?),
            v = pool.select_next_some() => v??,
        }
    }
}

async fn message_multiplexor<TBKAddrResolver>(
    rx: InstrumentedReceiver<(AccountId, Vec<u8>)>,
    btx: broadcast::Sender<Vec<u8>>,
    mut bp_resolver: TBKAddrResolver,
) -> anyhow::Result<()>
where
    TBKAddrResolver: Send + Sync + Clone + 'static + FnMut(AccountId) -> Option<String>,
{
    tracing::info!("Message multiplexor started");
    loop {
        let (node_id, raw_block) = rx.recv()?;
        let node_addr = bp_resolver(node_id);
        match btx.send(bincode::serialize(&(node_addr, raw_block))?) {
            Ok(number_subscribers) => {
                tracing::info!("Message received by {} subs", number_subscribers);
            }
            Err(_err) => {
                // NOTE: this is not a real error: e.g. if there're no receivers
            }
        }
    }
}
