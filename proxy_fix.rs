// GSC-FQ 代理服务器修复方案
//
// 问题：telnet 连接代理端口时卡住不动
// 原因：copy_bidirectional 在远程服务器不响应时会一直等待直到 5 分钟超时
// 解决：实现带超时的双向数据转发

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration};
use tokio::net::TcpStream;
use std::io;

/// 改进的数据转发函数，解决卡住问题
pub async fn forward_data_with_timeouts(
    mut client: TcpStream,
    mut remote: TcpStream,
) -> Result<(), Box<dyn std::error::Error>> {
    // 设置合理的超时时间
    const READ_TIMEOUT: Duration = Duration::from_secs(30);  // 30秒读超时
    const WRITE_TIMEOUT: Duration = Duration::from_secs(10); // 10秒写超时
    const IDLE_TIMEOUT: Duration = Duration::from_secs(60);  // 60秒空闲超时

    // 禁用 Nagle 算法以降低延迟
    client.set_nodelay(true)?;
    remote.set_nodelay(true)?;

    let (mut client_read, mut client_write) = client.split();
    let (mut remote_read, mut remote_write) = remote.split();

    let mut buffer = vec![0u8; 8192]; // 8KB 缓冲区
    let mut last_activity = std::time::Instant::now();

    loop {
        tokio::select! {
            // 客户端到远程服务器的数据传输
            result = timeout(READ_TIMEOUT, client_read.read(&mut buffer)) => {
                match result {
                    Ok(Ok(0)) => {
                        println!("客户端关闭连接");
                        break;
                    }
                    Ok(Ok(n)) => {
                        last_activity = std::time::Instant::now();
                        println!("从客户端读取 {} 字节", n);

                        match timeout(WRITE_TIMEOUT, remote_write.write_all(&buffer[..n])).await {
                            Ok(Ok(())) => {
                                println!("向远程服务器写入 {} 字节", n);
                            }
                            Ok(Err(e)) => {
                                eprintln!("写入远程服务器失败: {}", e);
                                return Err("远程写入失败".into());
                            }
                            Err(_) => {
                                eprintln!("写入远程服务器超时");
                                return Err("远程写入超时".into());
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut {
                            println!("客户端读取超时，检查空闲时间");
                            if last_activity.elapsed() > IDLE_TIMEOUT {
                                eprintln!("连接空闲超时");
                                break;
                            }
                            continue;
                        } else {
                            eprintln!("客户端读取错误: {}", e);
                            break;
                        }
                    }
                    Err(_) => {
                        eprintln!("客户端读取超时");
                        if last_activity.elapsed() > IDLE_TIMEOUT {
                            eprintln!("连接空闲超时");
                            break;
                        }
                        continue;
                    }
                }
            }

            // 远程服务器到客户端的数据传输
            result = timeout(READ_TIMEOUT, remote_read.read(&mut buffer)) => {
                match result {
                    Ok(Ok(0)) => {
                        println!("远程服务器关闭连接");
                        break;
                    }
                    Ok(Ok(n)) => {
                        last_activity = std::time::Instant::now();
                        println!("从远程服务器读取 {} 字节", n);

                        match timeout(WRITE_TIMEOUT, client_write.write_all(&buffer[..n])).await {
                            Ok(Ok(())) => {
                                println!("向客户端写入 {} 字节", n);
                            }
                            Ok(Err(e)) => {
                                eprintln!("写入客户端失败: {}", e);
                                return Err("客户端写入失败".into());
                            }
                            Err(_) => {
                                eprintln!("写入客户端超时");
                                return Err("客户端写入超时".into());
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut {
                            println!("远程服务器读取超时，检查空闲时间");
                            if last_activity.elapsed() > IDLE_TIMEOUT {
                                eprintln!("连接空闲超时");
                                break;
                            }
                            continue;
                        } else {
                            eprintln!("远程服务器读取错误: {}", e);
                            break;
                        }
                    }
                    Err(_) => {
                        eprintln!("远程服务器读取超时");
                        if last_activity.elapsed() > IDLE_TIMEOUT {
                            eprintln!("连接空闲超时");
                            break;
                        }
                        continue;
                    }
                }
            }

            // 空闲超时检查
            _ = tokio::time::sleep(IDLE_TIMEOUT) => {
                if last_activity.elapsed() > IDLE_TIMEOUT {
                    eprintln!("连接空闲超时");
                    break;
                }
            }
        }
    }

    println!("数据转发完成");
    Ok(())
}

/// 简单的修复版本：只需要修改超时时间
pub async fn quick_fix_forward_data(
    mut client: TcpStream,
    mut remote: TcpStream,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::copy_bidirectional;

    // 禁用 Nagle 算法
    client.set_nodelay(true)?;
    remote.set_nodelay(true)?;

    // 使用较短的超时时间（而不是 5 分钟）
    match timeout(Duration::from_secs(60), copy_bidirectional(&mut client, &mut remote)).await {
        Ok(result) => {
            result.map_err(|e| format!("双向复制失败: {}", e))?;
        }
        Err(_) => {
            eprintln!("数据转发超时 60 秒，关闭连接");
            return Err("数据转发超时".into());
        }
    }

    Ok(())
}