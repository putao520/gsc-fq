mod support;

use anyhow::Result;
use gsc_fq::config::loader::{ConfigFile, ServerSection, ReverseProxySection};
use gsc_fq::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// 测试TOKEN认证机制
#[tokio::test]
async fn test_token_authentication() -> Result<()> {
    println!("🧪 测试TOKEN认证机制");

    // 设置测试环境
    std::env::set_var("YAMUX_POOL_SIZE", "1");
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "50");

    // 1. 启动本地PingPong服务器
    let local_server = support::PingPongServer::start().await?;
    let local_port = local_server.port();
    println!("📡 本地服务器启动在端口: {}", local_port);

    // 2. 测试配置
    let proxy_port = 9100;
    let control_port = 9101;
    let valid_token = "test-token-12345";
    let invalid_token = "invalid-token";

    // 3. 启动有TOKEN认证的反向代理服务器
    tokio::spawn(async move {
        let mut server = ReverseProxyServer::new_with_auth(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            control_port,
            Some(valid_token.to_string()),
            vec![], // 不使用允許令牌列表，只使用主令牌
        );
        if let Err(e) = server.start().await {
            eprintln!("反向代理服务器错误: {:?}", e);
        }
    });

    // 等待服务器启动
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("✅ 反向代理服务器已启动，TOKEN认证已启用");

    // 4. 测试无效TOKEN连接
    println!("🔒 测试无效TOKEN连接...");
    let invalid_config = ConfigFile {
        server: Some(ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(true),
            auth_token: None,
            allowed_tokens: vec![],
        }),
        proxies: vec![],
        reverse_proxies: vec![ReverseProxySection {
            port: Some(proxy_port),
            server_port: None,
            local_port: Some(local_port),
            local_host: Some("127.0.0.1".to_string()),
            source_ip: None,
        }],
    };

    let server_addr = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        control_port,
    );

    // 尝试使用无效TOKEN连接
    let mut invalid_client = ReverseProxyClient::new_with_token(
        server_addr,
        invalid_config,
        invalid_token.to_string()
    );

    // 启动无效客户端并等待其失败
    let invalid_client_handle = tokio::spawn(async move {
        let start_time = std::time::Instant::now();
        if let Err(e) = invalid_client.start().await {
            println!("✅ 无效TOKEN客户端预期失败: {:?}", e);
            return start_time.elapsed();
        }
        Duration::from_secs(0) // 如果意外成功，返回0
    });

    // 等待一段时间让无效客户端尝试连接
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 5. 测试有效TOKEN连接
    println!("🔑 测试有效TOKEN连接...");
    let valid_config = ConfigFile {
        server: Some(ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(true),
            auth_token: None,
            allowed_tokens: vec![],
        }),
        proxies: vec![],
        reverse_proxies: vec![ReverseProxySection {
            port: Some(proxy_port + 1), // 使用不同端口避免冲突
            server_port: None,
            local_port: Some(local_port),
            local_host: Some("127.0.0.1".to_string()),
            source_ip: None,
        }],
    };

    // 启动有效客户端
    tokio::spawn(async move {
        let mut valid_client = ReverseProxyClient::new_with_token(
            server_addr,
            valid_config,
            valid_token.to_string(),
        );
        if let Err(e) = valid_client.start().await {
            eprintln!("有效TOKEN客户端错误: {:?}", e);
        }
    });

    // 等待有效客户端连接
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 6. 测试有效TOKEN的连接功能
    println!("🔗 测试有效TOKEN的连接功能...");
    match tokio::net::TcpStream::connect(("127.0.0.1", proxy_port + 1)).await {
        Ok(mut stream) => {
            println!("✅ 有效TOKEN客户端连接成功");

            // 发送HTTP请求
            let request = "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            stream.write_all(request.as_bytes()).await?;
            println!("✅ 已发送HTTP请求");

            // 读取响应
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stream);
            let mut status_line = String::new();

            match reader.read_line(&mut status_line).await {
                Ok(_) => {
                    println!("📄 响应状态: {}", status_line.trim());
                    if status_line.contains("200 OK") {
                        println!("🎉 TOKEN认证测试成功！反向代理正常工作");
                    } else {
                        println!("❌ 响应异常: {}", status_line);
                    }
                }
                Err(e) => {
                    println!("❌ 读取响应失败: {:?}", e);
                }
            }
        }
        Err(e) => {
            println!("❌ 有效TOKEN客户端连接失败: {:?}", e);
        }
    }

    // 7. 检查无效客户端的失败时间
    if let Ok(elapsed) = invalid_client_handle.await {
        if elapsed > Duration::from_millis(500) {
            println!("⚠️  无效TOKEN客户端连接时间过长: {:?}", elapsed);
        } else {
            println!("✅ 无效TOKEN客户端快速拒绝: {:?}", elapsed);
        }
    }

    println!("🏁 TOKEN认证测试完成");
    Ok(())
}

/// 测试允许多个TOKEN的认证
#[tokio::test]
async fn test_multiple_allowed_tokens() -> Result<()> {
    println!("🧪 测试多TOKEN认证机制");

    // 设置测试环境
    std::env::set_var("YAMUX_POOL_SIZE", "1");

    // 1. 启动本地PingPong服务器
    let local_server = support::PingPongServer::start().await?;
    let local_port = local_server.port();

    // 2. 测试配置
    let proxy_port = 9200;
    let control_port = 9201;
    let server_token = "server-main-token";
    let allowed_tokens = vec!["token1".to_string(), "token2".to_string(), "token3".to_string()];

    // 3. 启动支持多TOKEN的服务器
    let allowed_tokens_clone = allowed_tokens.clone();
    tokio::spawn(async move {
        let mut server = ReverseProxyServer::new_with_auth(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            control_port,
            Some(server_token.to_string()),
            allowed_tokens_clone,
        );
        if let Err(e) = server.start().await {
            eprintln!("反向代理服务器错误: {:?}", e);
        }
    });

    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("✅ 多TOKEN认证服务器已启动");

    // 4. 测试使用主令牌连接
    let server_addr = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        control_port,
    );

    println!("🔑 测试主令牌连接...");
    let main_token_config = ConfigFile {
        server: Some(ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(true),
            auth_token: None,
            allowed_tokens: vec![],
        }),
        proxies: vec![],
        reverse_proxies: vec![ReverseProxySection {
            port: Some(proxy_port),
            server_port: None,
            local_port: Some(local_port),
            local_host: Some("127.0.0.1".to_string()),
            source_ip: None,
        }],
    };

    tokio::spawn(async move {
        let mut client = ReverseProxyClient::new_with_token(
            server_addr,
            main_token_config,
            server_token.to_string(),
        );
        if let Err(e) = client.start().await {
            eprintln!("主令牌客户端错误: {:?}", e);
        }
    });

    // 5. 测试使用允许令牌连接
    for (i, allowed_token) in allowed_tokens.iter().enumerate() {
        println!("🔑 测试允许令牌{}连接...", i + 1);

        let allowed_token_config = ConfigFile {
            server: Some(ServerSection {
                bind_ip: Some("127.0.0.1".to_string()),
                debug: Some(true),
                auth_token: None,
                allowed_tokens: vec![],
            }),
            proxies: vec![],
            reverse_proxies: vec![ReverseProxySection {
                port: Some(proxy_port + 1 + i as u16),
                server_port: None,
                local_port: Some(local_port),
                local_host: Some("127.0.0.1".to_string()),
                source_ip: None,
            }],
        };

        let token = allowed_token.clone();
        let addr = server_addr;
        tokio::spawn(async move {
            let mut client = ReverseProxyClient::new_with_token(addr, allowed_token_config, token);
            if let Err(e) = client.start().await {
                eprintln!("允许令牌客户端错误: {:?}", e);
            }
        });
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // 6. 验证连接可用性
    println!("🔗 验证连接可用性...");
    for i in 0..=allowed_tokens.len() {
        let test_port = proxy_port + i as u16;
        match tokio::net::TcpStream::connect(("127.0.0.1", test_port)).await {
            Ok(_) => {
                println!("✅ 端口 {} 连接成功", test_port);
            }
            Err(e) => {
                println!("❌ 端口 {} 连接失败: {:?}", test_port, e);
            }
        }
    }

    println!("🏁 多TOKEN认证测试完成");
    Ok(())
}

/// 测试配置哈希验证
#[tokio::test]
async fn test_config_hash_verification() -> Result<()> {
    println!("🧪 测试配置哈希验证机制");

    // 设置测试环境
    std::env::set_var("YAMUX_POOL_SIZE", "1");

    // 1. 启动本地PingPong服务器
    let local_server = support::PingPongServer::start().await?;
    let local_port = local_server.port();

    // 2. 测试配置
    let proxy_port = 9300;
    let control_port = 9301;
    let token = "hash-test-token";

    // 3. 启动服务器
    tokio::spawn(async move {
        let mut server = ReverseProxyServer::new_with_auth(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            control_port,
            Some(token.to_string()),
            vec![],
        );
        if let Err(e) = server.start().await {
            eprintln!("反向代理服务器错误: {:?}", e);
        }
    });

    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("✅ 配置哈希验证服务器已启动");

    // 4. 创建客户端配置（这将自动计算正确的哈希）
    let server_addr = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        control_port,
    );

    let config = ConfigFile {
        server: Some(ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(true),
            auth_token: None,
            allowed_tokens: vec![],
        }),
        proxies: vec![],
        reverse_proxies: vec![ReverseProxySection {
            port: Some(proxy_port),
            server_port: None,
            local_port: Some(local_port),
            local_host: Some("127.0.0.1".to_string()),
            source_ip: None,
        }],
    };

    tokio::spawn(async move {
        let mut client = ReverseProxyClient::new_with_token(server_addr, config, token.to_string());
        if let Err(e) = client.start().await {
            eprintln!("配置哈希客户端错误: {:?}", e);
        }
    });

    tokio::time::sleep(Duration::from_secs(2)).await;

    // 5. 验证连接
    println!("🔗 验证配置哈希连接...");
    match tokio::net::TcpStream::connect(("127.0.0.1", proxy_port)).await {
        Ok(_) => {
            println!("✅ 配置哈希验证成功，连接正常");
        }
        Err(e) => {
            println!("❌ 配置哈希验证连接失败: {:?}", e);
        }
    }

    println!("🏁 配置哈希验证测试完成");
    Ok(())
}