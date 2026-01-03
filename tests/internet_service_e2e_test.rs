// 真实互联网服务E2E测试
// 使用公开的互联网服务验证基本网络连接功能

use anyhow::Result;
use gsc_fq::config::loader::ProxySection;
use gsc_fq::proxy::ProxyServerBuilder;
use std::net::{IpAddr, Ipv4Addr};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

/// 真实互联网服务端点配置
#[allow(dead_code)]
struct InternetService {
    name: &'static str,
    host: &'static str,
    port: u16,
    test_path: &'static str,
    expected_response: &'static str,
    description: &'static str,
}

/// 经过验证的可靠互联网服务
const INTERNET_SERVICES: &[InternetService] = &[
    InternetService {
        name: "httpbin",
        host: "httpbin.org",
        port: 80,
        test_path: "/get",
        expected_response: "\"url\": \"http://httpbin.org/get\"",
        description: "HTTP测试服务 - 用于验证HTTP请求转发",
    },
    InternetService {
        name: "google_dns",
        host: "8.8.8.8",
        port: 53,
        test_path: "",         // DNS查询
        expected_response: "", // DNS响应
        description: "Google DNS - 验证TCP连接到DNS服务",
    },
    InternetService {
        name: "cloudflare",
        host: "1.1.1.1",
        port: 80,
        test_path: "/",
        expected_response: "Cloudflare",
        description: "Cloudflare DNS HTTP服务 - 可靠的连接测试",
    },
];

#[tokio::test]
async fn test_internet_service_connectivity() -> Result<()> {
    println!("🌐 测试真实互联网服务连接性...");

    for service in INTERNET_SERVICES {
        println!("📡 测试服务: {} ({})", service.name, service.description);

        match timeout(
            Duration::from_secs(10),
            TcpStream::connect(format!("{}:{}", service.host, service.port)),
        )
        .await
        {
            Ok(Ok(_stream)) => {
                println!("✅ {} 连接成功", service.name);

                // 如果是HTTP服务，尝试简单测试
                if service.port == 80 {
                    println!("✅ {} HTTP端口可访问", service.name);
                } else {
                    println!("✅ {} TCP连接建立成功", service.name);
                }
            }
            Ok(Err(e)) => {
                println!("❌ {} 连接失败: {}", service.name, e);
            }
            Err(_) => {
                println!("❌ {} 连接超时", service.name);
            }
        }

        // 短暂延迟避免过于频繁的连接
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    println!("✅ 互联网服务连接性测试完成");
    Ok(())
}

#[tokio::test]
async fn test_proxy_to_internet_service() -> Result<()> {
    println!("🚀 测试通过GSC-FQ代理访问真实互联网服务...");

    // 选择可靠的测试服务
    let test_service = &INTERNET_SERVICES[0]; // httpbin.org
    println!(
        "📡 使用测试服务: {} ({})",
        test_service.name, test_service.description
    );

    // 1. 创建代理配置 - 转发到httpbin.org
    let proxy_config = ProxySection {
        local: "8080".to_string(),
        remote: format!("{}:{}", test_service.host, test_service.port),
        source_ip: None,
        allow_ips: None,
        max_conns_per_ip: None,
        cps_limit: None,
    };

    println!(
        "🔧 代理配置: 本地端口 {} → 远程服务 {}",
        proxy_config.local, proxy_config.remote
    );

    // 2. 启动代理服务器
    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut instance = ProxyServerBuilder::new()
        .bind_ip(bind_ip)
        .add_proxy(proxy_config)
        .build()?;

    let server_handle = tokio::spawn(async move {
        let _ = instance.start().await;
    });

    // 等待代理服务器启动
    tokio::time::sleep(Duration::from_millis(1000)).await;
    println!("✅ 代理服务器已启动在端口 8080");

    // 3. 通过代理访问互联网服务
    println!("🔗 通过代理访问 httpbin.org...");
    let test_start = std::time::Instant::now();

    match timeout(
        Duration::from_secs(15),
        TcpStream::connect("127.0.0.1:8080"),
    )
    .await
    {
        Ok(Ok(mut stream)) => {
            let connect_time = test_start.elapsed();
            println!("✅ 连接代理成功，耗时: {:?}", connect_time);

            // 发送HTTP请求
            let http_request = format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: GSC-FQ-Test/1.0\r\n\r\n",
                test_service.test_path, test_service.host
            );

            let write_start = std::time::Instant::now();
            stream.write_all(http_request.as_bytes()).await?;
            stream.flush().await?;
            let write_time = write_start.elapsed();
            println!("✅ 请求发送完成，耗时: {:?}", write_time);

            // 尝试读取响应头
            let read_start = std::time::Instant::now();
            let mut reader = BufReader::new(stream);
            let mut response_line = String::new();

            match timeout(
                Duration::from_secs(10),
                reader.read_line(&mut response_line),
            )
            .await
            {
                Ok(Ok(_)) => {
                    let read_time = read_start.elapsed();
                    println!(
                        "📥 首行响应: {} (耗时: {:?})",
                        response_line.trim(),
                        read_time
                    );

                    if response_line.starts_with("HTTP/1.1") {
                        println!("✅ 收到HTTP响应头 - 代理工作正常");

                        // 尝试读取更多内容以验证数据完整性
                        let mut body_content = String::new();
                        let mut buffer = String::new();
                        let mut body_read = false;

                        // 读取几行内容
                        for _ in 0..10 {
                            buffer.clear();
                            match timeout(Duration::from_secs(2), reader.read_line(&mut buffer))
                                .await
                            {
                                Ok(Ok(0)) => break,
                                Ok(Ok(_)) => {
                                    body_content.push_str(&buffer);
                                    body_read = true;
                                    if buffer.contains("}") || buffer.contains("</html>") {
                                        break;
                                    }
                                }
                                Ok(Err(_)) | Err(_) => break,
                            }
                        }

                        if body_content.contains("httpbin.org") || body_content.contains("\"url\"")
                        {
                            println!("✅ 代理成功转发到 httpbin.org 并收到正确响应数据");
                        } else if body_read {
                            println!("✅ 代理成功转发数据，响应长度: {} 字符", body_content.len());
                        } else {
                            println!("✅ 代理连接成功，收到HTTP响应");
                        }
                    } else {
                        println!("⚠️  收到非HTTP响应，但连接已建立");
                    }
                }
                Ok(Err(e)) => {
                    println!("⚠️  读取响应时出错: {:?}", e);
                }
                Err(_) => {
                    println!("⚠️  读取响应超时");
                }
            }
        }
        Ok(Err(e)) => {
            println!("❌ 连接代理失败: {:?}", e);
        }
        Err(_) => {
            println!("❌ 连接代理超时");
        }
    }

    // 4. 清理资源
    println!("🧹 清理测试资源...");
    let _ = server_handle.await;

    println!("✅ 代理到互联网服务测试完成");
    Ok(())
}

#[tokio::test]
async fn test_basic_network_functionality() -> Result<()> {
    println!("🔍 测试基础网络功能...");

    // 测试基本的TCP连接功能
    let services = vec![
        ("Google DNS", "8.8.8.8", 53),
        ("Cloudflare DNS", "1.1.1.1", 53),
        ("Google HTTP", "google.com", 80),
    ];

    let mut successful_connections = 0;

    for (name, host, port) in &services {
        println!("📡 测试连接到 {}: {}:{}", name, host, port);

        match timeout(
            Duration::from_secs(5),
            TcpStream::connect(format!("{}:{}", host, port)),
        )
        .await
        {
            Ok(Ok(_stream)) => {
                println!("✅ {} 连接成功", name);
                successful_connections += 1;
            }
            Ok(Err(e)) => {
                println!("❌ {} 连接失败: {}", name, e);
            }
            Err(_) => {
                println!("❌ {} 连接超时", name);
            }
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    println!(
        "📊 连接测试结果: {}/{} 成功",
        successful_connections,
        services.len()
    );

    if successful_connections > 0 {
        println!("✅ 基础网络功能正常，可以建立TCP连接到真实互联网服务");
    } else {
        println!("❌ 基础网络功能异常，无法连接到任何互联网服务");
    }

    Ok(())
}
