use std::time::Duration;

use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config =
        wtransport::ClientConfig::builder().with_bind_default().with_no_cert_validation().build();

    let endpoint =
        wtransport::Endpoint::client(config).with_context(|| "failed to build quic client")?;

    eprintln!("starting quic client");

    for i in 0.. {
        let url = "https://127.0.0.1:10003".to_string();
        eprintln!("{i:>10}: connecting to {url}");
        let connection = endpoint.connect(url).await.with_context(|| "failed to connect")?;

        let opening_uni_stream =
            connection.open_uni().await.with_context(|| "failed to open uni stream")?;

        {
            let mut stream =
                opening_uni_stream.await.with_context(|| "failed to accept uni stream")?;

            stream.write_all(b"hello").await.with_context(|| "failed to write")?;
            stream.finish().await.with_context(|| "failed to finish")?;

            eprintln!("sent hello");
        }

        eprintln!("{i:>10}: sleeping...");

        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    Ok(())
}
