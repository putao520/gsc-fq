# GSC-FQ 测试指南

## 1. 测试概述

GSC-FQ包含多层次的测试，确保代码质量和功能的正确性。

### 1.1 测试类型

| 测试类型 | 位置 | 命令 | 说明 |
|---------|------|-----|-----|
| 单元测试 | src/** | `cargo test --lib` | 库级别单元测试 |
| 集成测试 | tests/** | `cargo test --test` | 完整功能集成测试 |
| 基准测试 | benches/** | `cargo bench` | 性能基准测试 |
| 文档测试 | src/** | `cargo test --doc` | 文档中的代码示例 |

## 2. 单元测试

### 2.1 测试目标

单元测试用于测试独立的模块和函数。

#### Config 模块测试
- 配置文件加载和解析
- TOML格式验证
- 字段验证
- 边界情况处理

#### Error 模块测试
- 错误类型转换
- 错误消息生成
- 错误链传播

#### Proxy 模块测试
- ProxyServerBuilder构建
- 配置验证
- 连接处理逻辑

### 2.2 运行单元测试

```bash
# 运行所有单元测试
cargo test --lib

# 运行特定模块的测试
cargo test --lib config::
cargo test --lib proxy::
cargo test --lib error::

# 运行特定测试函数
cargo test --lib config::tests::test_load_config

# 显示测试输出
cargo test --lib -- --nocapture

# 运行单线程（调试用）
cargo test --lib -- --test-threads=1
```

### 2.3 编写单元测试

#### 测试模板

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // Arrange: 准备测试数据
        let input = ...;
        
        // Act: 执行被测代码
        let result = function_under_test(input);
        
        // Assert: 验证结果
        assert_eq!(result, expected);
    }
}
```

#### 测试示例

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_load_valid_toml() {
        let toml_content = r#"
        [server]
        bind_ip = "127.0.0.1"
        debug = false
        
        [[proxies]]
        local_port = 8080
        remote_host = "example.com"
        remote_port = 80
        "#;
        
        let config = ConfigLoader::load_from_string(toml_content);
        assert!(config.is_ok());
        
        let config = config.unwrap();
        assert_eq!(config.proxies.len(), 1);
    }

    #[test]
    fn test_invalid_port() {
        let toml_content = r#"
        [[proxies]]
        local_port = 99999
        remote_host = "example.com"
        remote_port = 80
        "#;
        
        let result = ConfigLoader::load_from_string(toml_content);
        assert!(result.is_err());
    }
}
```

## 3. 集成测试

### 3.1 测试位置

集成测试位于 `tests/` 目录：
- `tests/real_e2e_integration_test.rs` - 完整端到端测试
- `tests/simple_e2e_test.rs` - 简化的端到端测试

### 3.2 测试目标

- **代理功能测试**: 验证正向代理转发功能
- **黑洞检测测试**: 验证黑洞服务器检测
- **多规则测试**: 验证多个代理规则协同工作
- **高并发测试**: 验证高并发连接处理

### 3.3 运行集成测试

```bash
# 运行所有集成测试
cargo test --test '*'

# 运行特定集成测试
cargo test --test proxy_functionality_test
cargo test --test blackhole_functionality_test
cargo test --test real_e2e_integration_test
cargo test --test simple_e2e_test

# 显示输出
cargo test --test real_e2e_integration_test -- --nocapture

# 运行单线程
cargo test --test real_e2e_integration_test -- --test-threads=1
```

### 3.4 集成测试结构

```rust
#[tokio::test]
async fn test_proxy_forwarding() {
    // 1. 设置测试服务器
    let test_server = start_mock_server(9999).await;
    
    // 2. 配置代理规则
    let config = create_test_config(
        8888,
        "127.0.0.1",
        9999,
    );
    
    // 3. 启动代理服务器
    let mut proxy_server = ProxyServerBuilder::new()
        .bind_ip("127.0.0.1".parse().unwrap())
        .add_proxies(vec![config])
        .build()
        .unwrap();
    
    tokio::spawn(async move {
        let _ = proxy_server.start().await;
    });
    
    // 4. 允许服务器启动
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // 5. 发送测试请求
    let response = send_test_request("127.0.0.1:8888").await;
    
    // 6. 验证响应
    assert_eq!(response.status, 200);
}
```

## 4. 基准测试

### 4.1 基准目标

- 转发吞吐量（MB/s）
- 连接建立时间
- 内存使用情况
- CPU使用率

### 4.2 运行基准测试

```bash
# 运行所有基准测试
cargo bench

# 运行特定基准
cargo bench proxy_forwarding

# 生成HTML报告
cargo bench -- --verbose

# 对比基准结果
cargo bench -- --baseline <baseline_name>
```

### 4.3 编写基准测试

#### 基准测试模板

```rust
#[cfg(test)]
mod benches {
    use criterion::{black_box, criterion_group, criterion_main, Criterion};

    fn bench_forwarding(c: &mut Criterion) {
        c.bench_function("forward_1kb", |b| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async {
                    // 基准代码
                    forward_data(black_box(data)).await
                })
        });
    }

    criterion_group!(benches, bench_forwarding);
    criterion_main!(benches);
}
```

## 5. 代码覆盖率

### 5.1 生成覆盖率报告

```bash
# 使用tarpaulin生成覆盖率
cargo tarpaulin --out Html --output-dir coverage

# 使用llvm-cov
cargo llvm-cov --html
```

### 5.2 覆盖率目标

- 整体代码覆盖率: >= 80%
- 核心模块覆盖率: >= 90%
- 错误处理代码覆盖率: 100%

## 6. 测试工作流

### 6.1 本地开发流程

```bash
# 1. 编写新功能/修复
# ... edit code ...

# 2. 运行单元测试
cargo test --lib

# 3. 运行集成测试
cargo test --test '*'

# 4. 运行代码检查
cargo clippy

# 5. 代码格式化
cargo fmt

# 6. 文档测试
cargo test --doc

# 7. 基准测试（可选）
cargo bench
```

### 6.2 提交前检查清单

- [ ] 所有测试通过
- [ ] 代码通过clippy检查
- [ ] 代码已格式化
- [ ] 添加了新的测试用例
- [ ] 更新了文档
- [ ] 提交消息清晰有意义

## 7. 故障排查

### 7.1 常见测试问题

#### 异步测试超时

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_async_operation() {
    // 使用超时防止无限等待
    tokio::time::timeout(
        Duration::from_secs(5),
        async_function()
    ).await.expect("Operation timed out");
}
```

#### 端口冲突

```bash
# 使用随机端口避免冲突
cargo test -- --test-threads=1

# 或在测试中使用操作系统分配的端口
let listener = TcpListener::bind("127.0.0.1:0").await?;
let port = listener.local_addr()?.port();
```

#### 资源泄漏

```rust
// 确保正确清理资源
#[tokio::test]
async fn test_with_cleanup() {
    let resource = acquire_resource().await;
    defer! {
        release_resource(resource).await;
    }
    // 测试代码
}
```

### 7.2 调试测试

```bash
# 显示日志输出
RUST_LOG=debug cargo test -- --nocapture

# 运行单个测试，显示输出
cargo test test_name -- --nocapture --test-threads=1

# 运行并暂停
cargo test -- --nocapture --test-threads=1 -- wait_for_debugger()
```

## 8. CI/CD 集成

### 8.1 GitHub Actions 工作流

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      
      - name: Run tests
        run: cargo test --verbose
      
      - name: Run clippy
        run: cargo clippy -- -D warnings
      
      - name: Run fmt
        run: cargo fmt -- --check
```

### 8.2 本地模拟CI

```bash
#!/bin/bash
set -e

echo "Running unit tests..."
cargo test --lib

echo "Running integration tests..."
cargo test --test '*'

echo "Running clippy..."
cargo clippy -- -D warnings

echo "Checking fmt..."
cargo fmt -- --check

echo "All checks passed!"
```

## 9. 测试数据

### 9.1 测试配置示例

```toml
# config_test.toml
[server]
bind_ip = "127.0.0.1"
debug = true

[[proxies]]
local_port = 8080
remote_host = "127.0.0.1"
remote_port = 9999

[[proxies]]
local_port = 5432
remote_host = "db.example.com"
remote_port = 5432
source_ip = "10.0.0.1"

[[reverse_proxies]]
port = 8080
```

### 9.2 模拟服务器

```rust
async fn start_mock_server(port: u16) -> MockServer {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to bind");
    
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = vec![0; 1024];
                let _ = socket.read(&mut buf).await;
                let _ = socket.write_all(b"OK").await;
            });
        }
    });
    
    MockServer { port }
}
```

## 10. 最佳实践

### 10.1 测试编写

1. **单一责任**: 每个测试只测试一个功能
2. **清晰命名**: 测试名称应该说明测试的内容
3. **独立性**: 测试不应该相互依赖
4. **可重复性**: 测试应该每次都产生相同的结果
5. **快速**: 单元测试应该快速执行

### 10.2 测试维护

1. **及时更新**: 修改代码后更新对应的测试
2. **删除过时测试**: 移除不再相关的测试
3. **记录假设**: 在复杂测试中添加注释说明假设
4. **使用fixtures**: 复用测试数据和设置

---

**测试指南版本**: 1.0  
**最后更新**: 2024年11月  
