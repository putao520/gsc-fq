mod support;

use anyhow::Result;
use support::{
    pick_available_port, ProxyHandle, RemoteTarget, TestServer, LOCALHOST,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_proxy_forwards_data() -> Result<()> {
    let proxy_port = pick_available_port()?;
    let server = TestServer::start_echo().await?;
    let proxy =
        ProxyHandle::start(proxy_port, RemoteTarget::socket(server.addr())).await?;

    let mut stream = TcpStream::connect((LOCALHOST, proxy.local_port())).await?;
    let payload = b"ping through proxy";
    stream.write_all(payload).await?;

    let mut buf = vec![0u8; payload.len()];
    stream.read_exact(&mut buf).await?;
    assert_eq!(buf, payload);

    stream.shutdown().await?;
    proxy.shutdown().await;
    server.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_proxy_handles_multiple_messages() -> Result<()> {
    let proxy_port = pick_available_port()?;
    let server = TestServer::start_echo().await?;
    let proxy =
        ProxyHandle::start(proxy_port, RemoteTarget::socket(server.addr())).await?;
    let mut stream = TcpStream::connect((LOCALHOST, proxy.local_port())).await?;

    for idx in 0..3 {
        let payload = format!("block-{idx}");
        stream.write_all(payload.as_bytes()).await?;
        let mut buf = vec![0u8; payload.len()];
        stream.read_exact(&mut buf).await?;
        assert_eq!(buf, payload.as_bytes());
    }

    stream.shutdown().await?;
    proxy.shutdown().await;
    server.shutdown().await?;
    Ok(())
}
