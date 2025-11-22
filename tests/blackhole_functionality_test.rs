mod support;

use anyhow::Result;
use support::{pick_available_port, ProxyHandle, RemoteTarget, LOCALHOST};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_blackhole_discards_data_without_response() -> Result<()> {
    // Create a mock server that accepts connections but never responds
    let proxy_port = pick_available_port()?;
    let blackhole_server_port = pick_available_port()?;

    // Start a "blackhole" server that accepts but never responds
    let blackhole_handle = tokio::spawn(async move {
        let listener = TcpListener::bind((LOCALHOST, blackhole_server_port)).await.unwrap();
        loop {
            match listener.accept().await {
                Ok((mut stream, _)) => {
                    // Accept connection but never read or write anything
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        let _ = stream.shutdown().await;
                    });
                }
                Err(_) => break,
            }
        }
    });

    // Give blackhole server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    let proxy =
        ProxyHandle::start(proxy_port, RemoteTarget::localhost(blackhole_server_port)).await?;

    // Give more time for the proxy to be fully ready
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut stream = TcpStream::connect((LOCALHOST, proxy.local_port())).await?;
    stream.write_all(b"probe").await?;

    let mut buf = [0u8; 16];
    // Increase timeout for Windows which can be slower
    let read = timeout(Duration::from_millis(2000), stream.read(&mut buf)).await;
    assert!(read.is_err(), "blackhole should withhold responses");

    stream.write_all(b"still here").await?;
    stream.shutdown().await?;
    proxy.shutdown().await;

    // Stop the blackhole server
    blackhole_handle.abort();
    Ok(())
}
