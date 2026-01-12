// 跨平台零拷贝性能对比测试
//
// 测试目标：
// 1. 对比不同缓冲区大小的性能差异
// 2. 测试不同数据量的性能表现
// 3. 测量吞吐量和延迟

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use gsc_fq::proxy::zero_copy::{bulk_copy, bulk_copy_optimized};
use std::hint::black_box;
use std::io::Cursor;
use tokio::io::AsyncReadExt;

/// 模拟不同数据量的测试数据
fn create_test_data(size: usize) -> Vec<u8> {
    vec![42u8; size]
}

/// 基准实现：tokio::io::copy（默认 8KB 缓冲区）
fn benchmark_tokio_copy(data: &[u8]) -> u64 {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let reader = Cursor::new(data);
        let mut writer = Vec::new();

        tokio::io::copy(&mut reader.take(data.len() as u64), &mut writer)
            .await
            .unwrap();

        writer.len() as u64
    })
}

/// 我们的实现：bulk_copy (128KB)
fn benchmark_bulk_copy_128k(data: &[u8]) -> u64 {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let reader = Cursor::new(data);
        let mut writer = Vec::new();

        bulk_copy(reader, &mut writer).await.unwrap();

        writer.len() as u64
    })
}

/// 我们的实现：bulk_copy_optimized (256KB - macOS/其他平台)
fn benchmark_bulk_copy_256k(data: &[u8]) -> u64 {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let reader = Cursor::new(data);
        let mut writer = Vec::new();

        bulk_copy_optimized(reader, &mut writer, 256 * 1024).await.unwrap();

        writer.len() as u64
    })
}

/// 我们的实现：bulk_copy_optimized (512KB - Windows 平台)
fn benchmark_bulk_copy_512k(data: &[u8]) -> u64 {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let reader = Cursor::new(data);
        let mut writer = Vec::new();

        bulk_copy_optimized(reader, &mut writer, 512 * 1024).await.unwrap();

        writer.len() as u64
    })
}

/// 我们的实现：bulk_copy_optimized (1MB)
fn benchmark_bulk_copy_1m(data: &[u8]) -> u64 {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let reader = Cursor::new(data);
        let mut writer = Vec::new();

        bulk_copy_optimized(reader, &mut writer, 1024 * 1024).await.unwrap();

        writer.len() as u64
    })
}

/// 不同缓冲区大小的性能对比
fn bench_buffer_size_comparison(c: &mut Criterion) {
    let data = create_test_data(10 * 1024 * 1024); // 10MB

    let mut group = c.benchmark_group("buffer_size_comparison");
    group.throughput(Throughput::Bytes(10 * 1024 * 1024));
    group.sample_size(20); // 减少样本数，因为数据量大

    let buffer_sizes = vec![
        (8 * 1024, "8KB (tokio default)"),
        (64 * 1024, "64KB"),
        (128 * 1024, "128KB (Linux fallback)"),
        (256 * 1024, "256KB (macOS/general)"),
        (512 * 1024, "512KB (Windows)"),
        (1024 * 1024, "1MB"),
        (2 * 1024 * 1024, "2MB"),
    ];

    for (size, name) in buffer_sizes {
        group.bench_with_input(BenchmarkId::new(name, size), &size, |b, &buffer_size| {
            b.iter(|| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let reader = Cursor::new(&data);
                    let mut writer = Vec::new();
                    black_box(
                        bulk_copy_optimized(reader, &mut writer, buffer_size)
                            .await
                            .unwrap(),
                    );
                });
            });
        });
    }

    group.finish();
}

/// 不同数据量的性能对比
fn bench_data_size_comparison(c: &mut Criterion) {
    let data_sizes = vec![
        (100 * 1024, "100KB (小文件)"),
        (1024 * 1024, "1MB (中等文件)"),
        (10 * 1024 * 1024, "10MB (大文件)"),
    ];

    let mut group = c.benchmark_group("data_size_comparison");
    group.sample_size(30);

    for (size, name) in data_sizes {
        let data = create_test_data(size);
        group.throughput(Throughput::Bytes(size as u64));

        // tokio::io::copy (基准)
        group.bench_with_input(BenchmarkId::new("tokio_copy_8kb", name), &size, |b, &_s| {
            b.iter(|| black_box(benchmark_tokio_copy(&data)));
        });

        // bulk_copy 128KB
        group.bench_with_input(BenchmarkId::new("bulk_copy_128kb", name), &size, |b, &_s| {
            b.iter(|| black_box(benchmark_bulk_copy_128k(&data)));
        });

        // bulk_copy 256KB
        group.bench_with_input(BenchmarkId::new("bulk_copy_256kb", name), &size, |b, &_s| {
            b.iter(|| black_box(benchmark_bulk_copy_256k(&data)));
        });

        // bulk_copy 512KB
        group.bench_with_input(BenchmarkId::new("bulk_copy_512kb", name), &size, |b, &_s| {
            b.iter(|| black_box(benchmark_bulk_copy_512k(&data)));
        });

        // bulk_copy 1MB
        group.bench_with_input(BenchmarkId::new("bulk_copy_1mb", name), &size, |b, &_s| {
            b.iter(|| black_box(benchmark_bulk_copy_1m(&data)));
        });
    }

    group.finish();
}

/// 平台特定优化对比
fn bench_platform_optimization(c: &mut Criterion) {
    let scenarios = vec![
        (100 * 1024, "小文件_100KB"),
        (1024 * 1024, "中等文件_1MB"),
        (10 * 1024 * 1024, "大文件_10MB"),
    ];

    let mut group = c.benchmark_group("platform_optimization");

    for (size, name) in scenarios {
        let data = create_test_data(size);
        group.throughput(Throughput::Bytes(size as u64));

        // 基准：tokio::io::copy
        group.bench_with_input(BenchmarkId::new("tokio_copy", name), &size, |b, &_s| {
            b.iter(|| black_box(benchmark_tokio_copy(&data)));
        });

        // Linux 128KB
        group.bench_with_input(BenchmarkId::new("linux_128kb", name), &size, |b, &_s| {
            b.iter(|| black_box(benchmark_bulk_copy_128k(&data)));
        });

        // macOS 256KB
        group.bench_with_input(BenchmarkId::new("macos_256kb", name), &size, |b, &_s| {
            b.iter(|| black_box(benchmark_bulk_copy_256k(&data)));
        });

        // Windows 512KB
        group.bench_with_input(BenchmarkId::new("windows_512kb", name), &size, |b, &_s| {
            b.iter(|| black_box(benchmark_bulk_copy_512k(&data)));
        });
    }

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(std::time::Duration::from_secs(2))
        .measurement_time(std::time::Duration::from_secs(5));
    targets = bench_buffer_size_comparison, bench_data_size_comparison, bench_platform_optimization
);

criterion_main!(benches);
