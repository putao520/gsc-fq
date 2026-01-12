/// 连接池泄露修复验证测试
///
/// 测试目标：
/// 1. 验证连接池大小固定（不会无限增长）
/// 2. 验证连接被取走后自动补充
/// 3. 验证长时间运行不会 OOM

use gsc_fq::proxy::ConnectionPool;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::sleep;

#[tokio::test]
async fn test_connection_pool_fixed_size() {
    // 创建测试服务器
    let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    // 启动回显服务器
    tokio::spawn(async move {
        loop {
            if let Ok((mut socket, _)) = server.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match socket.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                if socket.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        }
    });

    sleep(Duration::from_millis(100)).await;

    // 创建连接池
    let pool = ConnectionPool::new(addr, None);
    pool.start().await.unwrap();

    // 等待预热完成
    sleep(Duration::from_secs(2)).await;

    let initial_stats = pool.get_stats();
    println!("初始统计: {:?}", initial_stats);

    // 模拟 100 次连接获取
    for i in 0..100 {
        match pool.acquire().await {
            Ok(_conn) => {
                // 模拟使用连接后直接关闭（不归还）
                drop(_conn);
                if i % 10 == 0 {
                    println!("已完成 {} 次连接", i);
                }
            }
            Err(e) => {
                eprintln!("第 {} 次获取连接失败: {}", i, e);
            }
        }

        // 每次间隔 50ms，给补充任务时间
        sleep(Duration::from_millis(50)).await;
    }

    // 等待补充任务完成
    sleep(Duration::from_secs(3)).await;

    let final_stats = pool.get_stats();
    println!("最终统计: {:?}", final_stats);

    // 验证：总创建的连接数应该接近初始池大小 + 补充的连接
    // 不应该远超池大小（之前会增长到 200+）
    let expected_max = 160; // 50（初始）+ 100（acquire）+ 10（缓冲）的合理上限
    assert!(
        final_stats.total_created <= expected_max,
        "连接池泄露！创建了 {} 个连接，期望 <= {}",
        final_stats.total_created,
        expected_max
    );

    // 验证连接成功率
    let total_attempts = final_stats.pool_hits + final_stats.pool_misses;
    let success_rate = if total_attempts > 0 {
        (final_stats.pool_hits as f64 / total_attempts as f64) * 100.0
    } else {
        0.0
    };

    println!("连接成功率: {:.1}%", success_rate);
    assert!(
        success_rate > 50.0,
        "连接成功率过低：{:.1}%",
        success_rate
    );

    pool.shutdown().await;
}

#[tokio::test]
async fn test_connection_pool_no_memory_leak() {
    // 创建测试服务器
    let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    // 启动回显服务器
    tokio::spawn(async move {
        loop {
            if let Ok((mut socket, _)) = server.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match socket.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                if socket.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        }
    });

    sleep(Duration::from_millis(100)).await;

    let pool = ConnectionPool::new(addr, None);
    pool.start().await.unwrap();

    // 记录初始内存
    let initial_mem = get_memory_usage().unwrap();
    println!("初始内存: {:.2} MB", initial_mem);

    // 大量连接操作
    for _ in 0..200 {
        if let Ok(conn) = pool.acquire().await {
            drop(conn); // 立即关闭
        }
        sleep(Duration::from_millis(10)).await;
    }

    // 等待补充和维护
    sleep(Duration::from_secs(5)).await;

    let final_mem = get_memory_usage().unwrap();
    let mem_increase = final_mem - initial_mem;
    println!("最终内存: {:.2} MB", final_mem);
    println!("内存增长: {:.2} MB", mem_increase);

    // 内存增长应该 < 20MB（50个连接的开销）
    assert!(
        mem_increase < 20.0,
        "内存泄露！增长了 {:.2} MB",
        mem_increase
    );

    pool.shutdown().await;
}

fn get_memory_usage() -> Result<f64, Box<dyn std::error::Error>> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let kb: u64 = parts[1].parse()?;
            return Ok(kb as f64 / 1024.0);
        }
    }
    Err("无法读取内存使用".into())
}
