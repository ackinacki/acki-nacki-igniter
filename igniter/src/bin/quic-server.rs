use anyhow::Context;
use tokio::io::AsyncReadExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let identity = wtransport::Identity::self_signed(["localhost"])?;
    let server_config = wtransport::ServerConfig::builder()
        .with_bind_address("127.0.0.1:10003".parse()?)
        .with_identity(identity)
        .build();
    let endpoint = wtransport::Endpoint::server(server_config)
        .with_context(|| "failed to build quic server")?;

    eprintln!("Server running, press ctrl+c to stop...");
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
