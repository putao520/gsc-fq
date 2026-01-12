// 快速性能对比测试
//
// 用于快速验证不同实现的性能差异

use gsc_fq::proxy::zero_copy::{bulk_copy, bulk_copy_optimized};
use std::io::Cursor;
use std::time::Instant;
use tokio::io::AsyncReadExt;

fn create_test_data(size: usize) -> Vec<u8> {
    vec![42u8; size]
}

#[test]
fn test_performance_comparison() {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("                 性能对比测试 - 快速验证");
    println!("═══════════════════════════════════════════════════════════════\n");

    // 测试不同数据量
    let test_sizes = vec![
        (100 * 1024, "100KB (小文件)"),
        (1024 * 1024, "1MB (中等文件)"),
        (10 * 1024 * 1024, "10MB (大文件)"),
    ];

    for (size, name) in test_sizes {
        println!("📊 测试数据量: {}", name);
        println!("{}\n", "─".repeat(60));

        let data = create_test_data(size);

        // tokio::io::copy (基准)
        let (bytes1, time1) = {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let start = Instant::now();
            let result = rt.block_on(async {
                let reader = Cursor::new(&data);
                let mut writer = Vec::new();
                tokio::io::copy(&mut reader.take(data.len() as u64), &mut writer)
                    .await
                    .unwrap();
                writer.len() as u64
            });
            (result, start.elapsed())
        };

        let throughput1 = (bytes1 as f64 / time1.as_secs_f64()) / (1024.0 * 1024.0);
        println!("  tokio::io::copy (8KB):");
        println!("    时间: {:>8.2?}", time1);
        println!("    吞吐量: {:>8.2} MB/s", throughput1);

        // bulk_copy 128KB (Linux fallback)
        let (bytes2, time2) = {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let start = Instant::now();
            let result = rt.block_on(async {
                let reader = Cursor::new(&data);
                let mut writer = Vec::new();
                bulk_copy(reader, &mut writer).await.unwrap();
                writer.len() as u64
            });
            (result, start.elapsed())
        };

        let throughput2 = (bytes2 as f64 / time2.as_secs_f64()) / (1024.0 * 1024.0);
        let speedup2 = time1.as_secs_f64() / time2.as_secs_f64();
        println!("  bulk_copy (128KB):");
        println!("    时间: {:>8.2?}", time2);
        println!("    吞吐量: {:>8.2} MB/s", throughput2);
        println!("    加速比: {:>8.2}x", speedup2);

        // bulk_copy 256KB (macOS/general)
        let (bytes3, time3) = {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let start = Instant::now();
            let result = rt.block_on(async {
                let reader = Cursor::new(&data);
                let mut writer = Vec::new();
                bulk_copy_optimized(reader, &mut writer, 256 * 1024).await.unwrap();
                writer.len() as u64
            });
            (result, start.elapsed())
        };

        let throughput3 = (bytes3 as f64 / time3.as_secs_f64()) / (1024.0 * 1024.0);
        let speedup3 = time1.as_secs_f64() / time3.as_secs_f64();
        println!("  bulk_copy (256KB):");
        println!("    时间: {:>8.2?}", time3);
        println!("    吞吐量: {:>8.2} MB/s", throughput3);
        println!("    加速比: {:>8.2}x", speedup3);

        // bulk_copy 512KB (Windows)
        let (bytes4, time4) = {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let start = Instant::now();
            let result = rt.block_on(async {
                let reader = Cursor::new(&data);
                let mut writer = Vec::new();
                bulk_copy_optimized(reader, &mut writer, 512 * 1024).await.unwrap();
                writer.len() as u64
            });
            (result, start.elapsed())
        };

        let throughput4 = (bytes4 as f64 / time4.as_secs_f64()) / (1024.0 * 1024.0);
        let speedup4 = time1.as_secs_f64() / time4.as_secs_f64();
        println!("  bulk_copy (512KB):");
        println!("    时间: {:>8.2?}", time4);
        println!("    吞吐量: {:>8.2} MB/s", throughput4);
        println!("    加速比: {:>8.2}x", speedup4);

        println!();
    }

    println!("═══════════════════════════════════════════════════════════════\n");
}

#[test]
fn test_platform_optimization_summary() {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("              平台优化策略总结");
    println!("═══════════════════════════════════════════════════════════════\n");

    println!("平台特定的优化策略:\n");
    println!("  🐧 Linux:");
    println!("     ├── splice() 系统调用 (真正的零拷贝，内核空间)");
    println!("     └── bulk_copy 128KB (fallback)");
    println!();
    println!("  🍎 macOS:");
    println!("     └── bulk_copy 256KB (大缓冲区优化)");
    println!();
    println!("  🪟 Windows:");
    println!("     └── bulk_copy 512KB (IOCP 友好)");
    println!();
    println!("  📦 其他平台:");
    println!("     └── bulk_copy 256KB (通用优化)");
    println!();

    // 快速测试
    let data = create_test_data(10 * 1024 * 1024);
    let (bytes, time) = {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let start = Instant::now();
        let result = rt.block_on(async {
            let reader = Cursor::new(&data);
            let mut writer = Vec::new();
            bulk_copy_optimized(reader, &mut writer, 256 * 1024).await.unwrap();
            writer.len() as u64
        });
        (result, start.elapsed())
    };

    let throughput = (bytes as f64 / time.as_secs_f64()) / (1024.0 * 1024.0);

    println!("快速测试结果 (10MB):");
    println!("  传输时间: {:?}", time);
    println!("  吞吐量: {:.2} MB/s", throughput);
    println!();
    println!("═══════════════════════════════════════════════════════════════\n");
}
