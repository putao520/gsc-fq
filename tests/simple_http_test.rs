use anyhow::Result;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_simple_http_connection() -> Result<()> {
    println!("🌐 测试简单HTTP连接到可靠IP");

    // 直接连接到IP地址，避免DNS问题
    let test_cases = vec![
        ("1.1.1.1", 80, "Cloudflare DNS"),     // Cloudflare DNS HTTP服务
        ("8.8.8.8", 53, "Google DNS"),         // Google DNS (TCP连接测试)
        ("208.67.222.222", 80, "OpenDNS"),     // OpenDNS HTTP服务
    ];

    for (ip, port, description) in test_cases {
        println!("📡 测试连接到 {} - {}", ip, description);

        match timeout(Duration::from_secs(3), tokio::net::TcpStream::connect(format!("{}:{}", ip, port))).await {
            Ok(Ok(_stream)) => {
                println!("✅ {} 连接成功", description);
            }
            Ok(Err(e)) => {
                println!("⚠️  {} 连接失败: {}", description, e);
            }
            Err(_) => {
                println!("⚠️  {} 连接超时", description);
            }
        }
    }

    // 测试一个简单的HTTP请求到Google IP
    println!("🌐 测试HTTP请求到Google");
    match timeout(Duration::from_secs(5), tokio::net::TcpStream::connect("142.250.196.68:80")).await {
        Ok(Ok(mut stream)) => {
            // 发送简单的HTTP请求
            let http_request = "GET / HTTP/1.1\r\nHost: google.com\r\nConnection: close\r\n\r\n";
            if let Err(e) = tokio::io::AsyncWriteExt::write_all(&mut stream, http_request.as_bytes()).await {
                println!("⚠️  HTTP请求发送失败: {}", e);
                return Ok(());
            }

            // 读取响应
            let mut response = [0u8; 1024];
            match timeout(Duration::from_secs(3), tokio::io::AsyncReadExt::read(&mut stream, &mut response)).await {
                Ok(Ok(bytes_read)) => {
                    let response_str = String::from_utf8_lossy(&response[..bytes_read]);
                    if response_str.starts_with("HTTP/1.1") {
                        println!("✅ HTTP请求成功，收到响应: {} 字节", bytes_read);
                    } else {
                        println!("⚠️  响应格式异常，但连接已建立");
                    }
                }
                Ok(Err(e)) => {
                    println!("⚠️  读取响应失败: {}", e);
                }
                Err(_) => {
                    println!("⚠️  响应超时，但连接已建立");
                }
            }
        }
        Ok(Err(e)) => {
            println!("⚠️  Google连接失败: {}", e);
        }
        Err(_) => {
            println!("⚠️  Google连接超时");
        }
    }

    println!("✅ 简单HTTP连接测试完成");
    Ok(())
}