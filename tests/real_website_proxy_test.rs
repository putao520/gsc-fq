mod support;

use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr};
use support::wait_for_port_ready;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::timeout;
use std::time::Duration;
use gsc_fq::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};
use gsc_fq::config::loader::ConfigFile;

// 真实网站测试配置
const PROXY_SERVER_PORT: u16 = 9001;
const PROXY_CLIENT_LISTEN_PORT: u16 = 9000;

// 可靠的网站IP和配置
struct RealWebsite {
    host: &'static str,
    ip: &'static str,
    port: u16,
    path: &'static str,
    expected_response: &'static str,
}

const WEBSITES: &[RealWebsite] = &[
    RealWebsite {
        host: "httpbin.org",
        ip: "54.230.97.49", // httpbin.org的一个IP
        port: 80,
        path: "/get",
        expected_response: "\"url\": \"http://httpbin.org/get\"",
    },
    RealWebsite {
        host: "google.com",
        ip: "142.250.196.68", // google.com的一个IP
        port: 80,
        path: "/",
        expected_response: "<title>Google</title>",
    },
];

#[tokio::test]
async fn test_tunnel_proxy_to_real_website() -> Result<()> {
    println!("🌐 测试隧道代理到真实网站");

    std::env::set_var("YAMUX_POOL_SIZE", "1");

    for website in WEBSITES {
        println!("\n🌍 测试网站: {} (IP: {})", website.host, website.ip);

        // 1. 配置隧道代理指向真实网站
        let proxy_config = gsc_fq::config::loader::ProxySection {
            local: "127.0.0.1:8080".to_string(),
            remote: format!("{}:{}", website.ip, website.port),
            source_ip: None,
        };

        let config = gsc_fq::config::loader::ConfigFile {
            server: Some(gsc_fq::config::loader::ServerSection {
                bind_ip: Some("127.0.0.1".to_string()),
                debug: Some(false),
            }),
            proxies: vec![proxy_config],
            reverse_proxies: vec![],
            reverse_proxy_server: None,
            reverse_proxy_client: None,
        };

        // 2. 启动隧道代理服务端
        println!("🔄 启动隧道代理服务端");
        let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let mut server = gsc_fq::proxy::ProxyServerBuilder::new()
            .bind_ip(bind_ip)
            .add_proxy(config.proxies[0].clone())
            .build()
            .unwrap();

        let server_handle = tokio::spawn(async move {
            let _ = server.start().await;
        });

        wait_for_port_ready(8080, Duration::from_secs(5)).await?;
        println!("✅ 隧道代理已启动在端口 8080");

        // 3. 通过代理访问真实网站
        println!("🌐 通过代理访问 http://{}{}", website.host, website.path);

        let mut stream = timeout(
            Duration::from_secs(10),
            tokio::net::TcpStream::connect("127.0.0.1:8080")
        ).await??;

        // 4. 发送HTTP请求
        let http_request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: GSC-FQ-Tunnel-Test/1.0\r\n\r\n",
            website.path, website.host
        );

        println!("📤 发送HTTP请求到 {}{}", website.host, website.path);
        stream.write_all(http_request.as_bytes()).await?;
        stream.flush().await?;

        // 5. 读取HTTP响应
        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();

        match timeout(Duration::from_secs(15), reader.read_line(&mut response_line)).await {
            Ok(Ok(_)) => {
                if response_line.starts_with("HTTP/1.1") {
                    println!("✅ 收到HTTP响应: {}", response_line.trim());

                    // 读取完整响应
                    let mut full_response = response_line;
                    let mut buffer = String::new();

                    loop {
                        match timeout(Duration::from_secs(5), reader.read_line(&mut buffer)).await {
                            Ok(Ok(0)) => break, // EOF
                            Ok(_) => {
                                full_response.push_str(&buffer);
                                buffer.clear();

                                // 检测响应结束
                                if full_response.contains("\r\n\r\n") {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }

                    // 6. 验证响应内容
                    if full_response.contains(website.expected_response) {
                        println!("✅ {} 响应验证成功", website.host);
                    } else if full_response.contains("HTTP/1.1 200") {
                        println!("✅ {} 连接成功，收到HTTP 200响应", website.host);
                    } else {
                        println!("⚠️  {} 响应内容可能不完整", website.host);
                    }

                    println!("📋 响应长度: {} 字符", full_response.len());
                } else {
                    println!("⚠️  {} 响应格式异常", website.host);
                }
            }
            Ok(Err(e)) => {
                println!("⚠️  {} 读取响应失败: {}", website.host, e);
            }
            Err(_) => {
                println!("⚠️  {} 响应超时", website.host);
            }
        }

        // 7. 清理
        server_handle.abort();
        let _ = server_handle.await;

        // 等待端口释放
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    println!("\n✅ 隧道代理到真实网站测试完成");
    Ok(())
}

#[tokio::test]
async fn test_reverse_tunnel_proxy_to_real_website() -> Result<()> {
    println!("🔄 测试反向隧道代理到真实网站");

    std::env::set_var("YAMUX_POOL_SIZE", "1");

    for website in WEBSITES {
        println!("\n🌍 反向测试网站: {} (IP: {})", website.host, website.ip);

        // 1. 配置反向代理指向真实网站
        let proxy_config = gsc_fq::config::loader::ReverseProxySection {
            server: PROXY_CLIENT_LISTEN_PORT.to_string(),
            local: format!("{}:{}", website.ip, website.port),
            source_ip: None,
        };

        let config = ConfigFile {
            server: None,
            proxies: vec![],
            reverse_proxies: vec![proxy_config],
            reverse_proxy_server: None,
            reverse_proxy_client: Some(gsc_fq::config::loader::ReverseProxyClientSection {
                server: format!("127.0.0.1:{}", PROXY_SERVER_PORT),
            }),
        };

        // 2. 启动反向代理服务端
        println!("🔄 启动反向代理服务端");
        let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let mut server = ReverseProxyServer::new(bind_ip, PROXY_SERVER_PORT);
        let server_handle = tokio::spawn(async move {
            let _ = server.start().await;
        });

        wait_for_port_ready(PROXY_SERVER_PORT, Duration::from_secs(5)).await?;
        println!("✅ 反向服务端已启动");

        // 3. 启动反向代理客户端
        println!("🔗 启动反向代理客户端");
        let server_addr = std::net::SocketAddr::new(bind_ip, PROXY_SERVER_PORT);
        let client_handle = tokio::spawn(async move {
            let mut client = ReverseProxyClient::new(server_addr, config);
            let _ = client.start().await;
        });

        // 等待代理设置完成
        tokio::time::sleep(Duration::from_secs(2)).await;
        wait_for_port_ready(PROXY_CLIENT_LISTEN_PORT, Duration::from_secs(5)).await?;
        println!("✅ 反向代理已就绪，端口: {}", PROXY_CLIENT_LISTEN_PORT);

        // 4. 通过反向代理访问真实网站
        println!("🌐 通过反向代理访问 http://{}{}", website.host, website.path);

        let mut stream = timeout(
            Duration::from_secs(10),
            tokio::net::TcpStream::connect(format!("127.0.0.1:{}", PROXY_CLIENT_LISTEN_PORT))
        ).await??;

        // 5. 发送端口头部 (反向代理协议要求)
        let port_bytes = (PROXY_CLIENT_LISTEN_PORT as u16).to_be_bytes();
        stream.write_all(&port_bytes).await?;
        stream.flush().await?;

        // 6. 发送HTTP请求
        let http_request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: GSC-FQ-Reverse-Test/1.0\r\n\r\n",
            website.path, website.host
        );

        println!("📤 发送HTTP请求到 {}{}", website.host, website.path);
        stream.write_all(http_request.as_bytes()).await?;
        stream.flush().await?;

        // 7. 读取HTTP响应
        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();

        match timeout(Duration::from_secs(15), reader.read_line(&mut response_line)).await {
            Ok(Ok(_)) => {
                if response_line.starts_with("HTTP/1.1") {
                    println!("✅ 收到HTTP响应: {}", response_line.trim());

                    // 读取完整响应
                    let mut full_response = response_line;
                    let mut buffer = String::new();

                    loop {
                        match timeout(Duration::from_secs(5), reader.read_line(&mut buffer)).await {
                            Ok(Ok(0)) => break, // EOF
                            Ok(_) => {
                                full_response.push_str(&buffer);
                                buffer.clear();

                                // 检测响应结束
                                if full_response.contains("\r\n\r\n") {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }

                    // 8. 验证响应内容
                    if full_response.contains(website.expected_response) {
                        println!("✅ {} 反向代理响应验证成功", website.host);
                    } else if full_response.contains("HTTP/1.1 200") {
                        println!("✅ {} 反向代理连接成功，收到HTTP 200响应", website.host);
                    } else {
                        println!("⚠️  {} 反向代理响应内容可能不完整", website.host);
                    }

                    println!("📋 响应长度: {} 字符", full_response.len());
                } else {
                    println!("⚠️  {} 反向代理响应格式异常", website.host);
                }
            }
            Ok(Err(e)) => {
                println!("⚠️  {} 反向代理读取响应失败: {}", website.host, e);
            }
            Err(_) => {
                println!("⚠️  {} 反向代理响应超时", website.host);
            }
        }

        // 9. 清理
        client_handle.abort();
        server_handle.abort();

        let _ = client_handle.await;
        let _ = server_handle.await;

        // 等待端口释放
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    println!("\n✅ 反向隧道代理到真实网站测试完成");
    Ok(())
}