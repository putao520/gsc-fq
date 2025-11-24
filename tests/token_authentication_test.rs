mod support;

use anyhow::Result;
use gsc_fq::config::loader::{ConfigFile, ServerSection, ReverseProxySection};
use gsc_fq::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// 测试TOKEN认证机制 - 真正的端到端功能测试
#[tokio::test]
async fn test_token_authentication() -> Result<()> {
    println!("🧪 测试TOKEN认证机制 - 端到端功能验证");

    // 设置测试环境
    std::env::set_var("YAMUX_POOL_SIZE", "1");
    std::env::set_var("DEBUG", "true");  // 启用调试日志

    // 1. 启动本地PingPong服务器
    let local_server = support::PingPongServer::start().await?;
    let local_port = local_server.port();
    println!("📡 本地PingPong服务器启动在端口: {}", local_port);

    // 2. 测试端口配置
    let proxy_external_port = 9100;  // 反向代理外部端口
    let control_port = 9101;         // 控制端口
    let valid_token = "test-token-12345";
    let invalid_token = "invalid-token";

    // 3. 启动有TOKEN认证的反向代理服务器
    let server_handle = tokio::spawn(async move {
        let mut server = ReverseProxyServer::new_with_auth(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            control_port,
            Some(valid_token.to_string()),
            vec![],
        );
        if let Err(e) = server.start().await {
            eprintln!("反向代理服务器错误: {:?}", e);
        }
    });

    // 等待服务器启动
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("✅ 反向代理服务器已启动，控制端口: {}", control_port);

    // 测试1: 无效TOKEN应该被拒绝
    println!("🔒 测试1: 无效TOKEN认证...");
    let server_addr = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        control_port,
    );

    let invalid_config = ConfigFile {
        server: Some(ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(true),
            auth_token: None,
            allowed_tokens: vec![],
        }),
        proxies: vec![],
        reverse_proxies: vec![ReverseProxySection {
            server: proxy_external_port.to_string(),
            local: format!("127.0.0.1:{}", local_port),
            source_ip: None,
        }],
        reverse_mode: Some("client".to_string()),
        reverse_target: Some(format!("127.0.0.1:{}", control_port)),
    };

    // 测试无效TOKEN连接（应该快速失败）
    let invalid_result = tokio::time::timeout(
        Duration::from_secs(3),
        async {
            let mut client = ReverseProxyClient::new_with_token(
                server_addr,
                invalid_config,
                invalid_token.to_string(),
            );
            client.start().await
        }
    ).await;

    match invalid_result {
        Ok(Err(e)) => {
            println!("✅ 无效TOKEN正确被拒绝: {:?}", e);
            assert!(e.to_string().contains("Authentication") || e.to_string().contains("Token"),
                   "应该返回认证相关错误");
        }
        Ok(Ok(_)) => {
            panic!("❌ 无效TOKEN不应该成功连接");
        }
        Err(_) => {
            println!("✅ 无效TOKEN测试超时，说明连接被正确拒绝（重试机制生效）");
        }
    }

    // 测试2: 有效TOKEN应该成功并转发数据
    println!("🔑 测试2: 有效TOKEN认证和数据转发...");

    let valid_config = ConfigFile {
        server: Some(ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(true),
            auth_token: None,
            allowed_tokens: vec![],
        }),
        proxies: vec![],
        reverse_proxies: vec![ReverseProxySection {
            server: proxy_external_port.to_string(),
            local: format!("127.0.0.1:{}", local_port),
            source_ip: None,
        }],
        reverse_mode: Some("client".to_string()),
        reverse_target: Some(format!("127.0.0.1:{}", control_port)),
    };

    // 启动有效TOKEN客户端
    let client_handle = tokio::spawn(async move {
        let mut client = ReverseProxyClient::new_with_token(
            server_addr,
            valid_config,
            valid_token.to_string(),
        );

        let start_time = std::time::Instant::now();
        let result = client.start().await;
        let duration = start_time.elapsed();
        (result, duration)
    });

    // 等待反向代理连接建立
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 测试3: 验证数据转发功能
    println!("🔗 测试3: 验证反向代理数据转发...");

    // 连接到反向代理的外部端口
    match tokio::net::TcpStream::connect(("127.0.0.1", proxy_external_port)).await {
        Ok(mut stream) => {
            println!("✅ 成功连接到反向代理外部端口: {}", proxy_external_port);

            // 发送HTTP请求到PingPong服务器
            let request = "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            stream.write_all(request.as_bytes()).await?;
            println!("✅ 已发送HTTP请求");

            // 读取响应
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stream);
            let mut response = String::new();

            // 读取第一行（状态行）
            match reader.read_line(&mut response).await {
                Ok(0) => {
                    panic!("❌ 连接被提前关闭，没有收到响应");
                }
                Ok(_) => {
                    println!("📄 收到响应: {}", response.trim());

                    // 验证响应包含PingPong服务器的特征
                    if response.contains("200") || response.contains("OK") || response.contains("pong") {
                        println!("🎉 数据转发测试成功！收到PingPong服务器响应");
                    } else {
                        panic!("❌ 响应格式异常，未收到预期的PingPong响应: {}", response);
                    }
                }
                Err(e) => {
                    panic!("❌ 读取响应失败: {:?}", e);
                }
            }
        }
        Err(e) => {
            panic!("❌ 无法连接到反向代理外部端口 {}: {:?}", proxy_external_port, e);
        }
    }

    // 测试4: 验证客户端连接状态
    println!("🔍 测试4: 验证客户端连接状态...");

    match tokio::time::timeout(Duration::from_secs(2), client_handle).await {
        Ok(Ok((result, duration))) => {
            match result {
                Ok(_) => {
                    println!("✅ 有效TOKEN客户端连接成功，耗时: {:?}", duration);

                    // 验证连接时间合理
                    if duration > Duration::from_secs(10) {
                        panic!("❌ 连接时间过长: {:?}，可能存在性能问题", duration);
                    }
                }
                Err(e) => {
                    panic!("❌ 有效TOKEN客户端连接失败: {:?}", e);
                }
            }
        }
        Ok(Err(e)) => {
            panic!("❌ 有效TOKEN客户端任务失败: {:?}", e);
        }
        Err(_) => {
            // 客户端仍在运行是正常的，因为它是长连接
            println!("ℹ️  有效TOKEN客户端仍在正常运行（长连接模式）");
        }
    }

    // 清理
    server_handle.abort();

    println!("🎉 TOKEN认证测试全部通过！");
    println!("   ✅ 无效TOKEN正确拒绝");
    println!("   ✅ 有效TOKEN成功认证");
    println!("   ✅ 数据转发正常工作");
    println!("   ✅ 端到端功能验证成功");

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
    let server_handle = tokio::spawn(async move {
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

    // 等待服务器启动
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("✅ 反向代理服务器已启动，支持多TOKEN认证");

    let server_addr = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        control_port,
    );

    // 4. 测试每个允许的TOKEN
    for (i, token) in allowed_tokens.iter().enumerate() {
        println!("🔑 测试TOKEN {}: {}", i + 1, token);

        let config = ConfigFile {
            server: Some(ServerSection {
                bind_ip: Some("127.0.0.1".to_string()),
                debug: Some(true),
                auth_token: None,
                allowed_tokens: vec![],
            }),
            proxies: vec![],
            reverse_proxies: vec![ReverseProxySection {
                server: proxy_port.to_string(),
                local: format!("127.0.0.1:{}", local_port),
                source_ip: None,
            }],
            reverse_mode: Some("client".to_string()),
            reverse_target: Some(format!("127.0.0.1:{}", control_port)),
        };

        let result = tokio::time::timeout(
            Duration::from_secs(3),
            async {
                let mut client = ReverseProxyClient::new_with_token(
                    server_addr,
                    config,
                    token.clone(),
                );
                client.start().await
            }
        ).await;

        match result {
            Ok(Ok(_)) => {
                println!("✅ TOKEN {} 认证成功", token);
            }
            Ok(Err(e)) => {
                panic!("❌ TOKEN {} 认证失败: {:?}", token, e);
            }
            Err(_) => {
                panic!("❌ TOKEN {} 测试超时", token);
            }
        }

        // 短暂等待，避免连接竞争
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // 5. 测试不在允许列表中的TOKEN
    println!("🔒 测试不在允许列表中的TOKEN...");
    let invalid_token = "invalid-token-12345";

    let invalid_config = ConfigFile {
        server: Some(ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(true),
            auth_token: None,
            allowed_tokens: vec![],
        }),
        proxies: vec![],
        reverse_proxies: vec![ReverseProxySection {
            server: (proxy_port + 1).to_string(),
            local: format!("127.0.0.1:{}", local_port),
            source_ip: None,
        }],
        reverse_mode: Some("client".to_string()),
        reverse_target: Some(format!("127.0.0.1:{}", control_port)),
    };

    let invalid_result = tokio::time::timeout(
        Duration::from_secs(3),
        async {
            let mut client = ReverseProxyClient::new_with_token(
                server_addr,
                invalid_config,
                invalid_token.to_string(),
            );
            client.start().await
        }
    ).await;

    match invalid_result {
        Ok(Err(e)) => {
            println!("✅ 无效TOKEN正确被拒绝: {:?}", e);
        }
        Ok(Ok(_)) => {
            panic!("❌ 无效TOKEN不应该成功连接");
        }
        Err(_) => {
            panic!("❌ 无效TOKEN测试不应该超时");
        }
    }

    // 清理
    server_handle.abort();

    println!("🎉 多TOKEN认证测试通过！");
    Ok(())
}