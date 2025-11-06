mod support;

use anyhow::Result;
use support::{pick_available_port, ProxyHandle, RemoteTarget, LOCALHOST};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_blackhole_discards_data_without_response() -> Result<()> {
    let proxy_port = pick_available_port()?;
    let unreachable_port = pick_available_port()?;
    let proxy =
        ProxyHandle::start(proxy_port, RemoteTarget::localhost(unreachable_port)).await?;
    let mut stream = TcpStream::connect((LOCALHOST, proxy.local_port())).await?;
    stream.write_all(b"probe").await?;

    let mut buf = [0u8; 16];
    let read = timeout(Duration::from_millis(100), stream.read(&mut buf)).await;
    assert!(read.is_err(), "blackhole should withhold responses");

    stream.write_all(b"still here").await?;
    stream.shutdown().await?;
    proxy.shutdown().await;
    Ok(())
}
