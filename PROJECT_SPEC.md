# GSC-FQ 项目规范文档

## 1. 项目概述

**项目名称**: GSC-FQ (Global Stream Control - Fast Forwarding and Query)

**版本**: 0.4.0

**类型**: 高性能TCP数据流代理转发CLI工具

**语言**: Rust

**许可证**: MIT OR Apache-2.0

**仓库**: https://github.com/putao520/gsc-fq

### 1.1 项目定位

GSC-FQ是一个由Rust + Tokio异步运行时构建的高性能TCP代理转发工具，提供灵活的TOML配置系统、智能调试系统，以及高效的数据转发能力。

## 2. 核心功能

### 2.1 主要特性

1. **高性能转发**: 
   - 基于Tokio异步运行时
   - 支持数千并发连接
   - 零拷贝优化
   - 高效内存管理

2. **灵活配置**:
   - TOML配置文件格式
   - 自动从`default.toml`加载
   - 支持多个代理规则
   - 支持源IP地址控制

3. **调试系统**:
   - 零开销调试模式
   - 通过配置文件控制
   - 完整的日志输出

4. **多种工作模式**:
   - 正向代理模式（Forward Proxy）
   - 反向代理服务器模式（Reverse Proxy Server）
   - 反向代理客户端模式（Reverse Proxy Client）

5. **系统管理**:
   - 优雅关闭
   - 信号处理
   - 资源清理

### 2.2 功能详解

#### 正向代理模式（默认）
- 读取`default.toml`配置文件
- 根据配置启动多个代理监听端口
- 将客户端连接转发到配置的远程主机
- 支持自定义源IP地址

#### 反向代理服务器模式
```bash
gsc-fq s <PORT>
```
- 在指定端口运行反向代理服务器
- 等待反向代理客户端连接
- 接收客户端的代理规则
- 执行转发任务

#### 反向代理客户端模式
```bash
gsc-fq c <ADDRESS:PORT>
```
- 连接到反向代理服务器
- 从配置文件读取反向代理规则
- 向服务器发送代理规则
- 处理转发连接

## 3. 架构设计

### 3.1 模块结构

```
src/
├── main.rs              # 程序入口，模式选择和启动
├── lib.rs               # 库入口
├── config/              # 配置系统
│   ├── loader.rs        # 配置文件加载
│   └── validator.rs     # 配置验证
├── error/               # 错误处理
│   └── mod.rs           # 错误类型定义
├── proxy/               # 正向代理核心
│   ├── server.rs        # 代理服务器
│   ├── handler.rs       # 连接处理
│   └── mod.rs           # 导出接口
├── reverse_proxy/       # 反向代理实现
│   ├── server.rs        # 反向代理服务器
│   ├── client.rs        # 反向代理客户端
│   └── protocol.rs      # 反向代理协议
└── utils/               # 工具模块
    ├── debug.rs         # 调试系统
    ├── system.rs        # 系统检查
    └── mod.rs           # 导出接口
```

### 3.2 依赖关系

**核心依赖**:
- **tokio**: 异步运行时 (1.40+, full features)
- **toml**: TOML配置解析 (0.8)
- **serde**: 序列化/反序列化 (1.0)
- **socket2**: 底层socket操作 (0.5)
- **nix**: Unix系统调用 (0.29)

**错误处理**:
- **thiserror**: 错误定义 (1.0)
- **anyhow**: 错误处理 (1.0)

**日志和调试**:
- **env_logger**: 日志系统 (0.11)

**并发工具**:
- **tokio-util**: Tokio工具库 (0.7)
- **yamux**: 多路复用 (0.10)
- **futures**: 异步计算 (0.3)

**序列化**:
- **bytes**: 字节缓冲 (1.5)
- **bincode**: 二进制序列化 (1.3)

**其他**:
- **num_cpus**: CPU信息 (1.16)
- **uuid**: UUID生成 (1.0)

## 4. 配置系统

### 4.1 配置文件格式

配置文件名称：`default.toml`

#### 服务器部分 (Server Section)
```toml
[server]
bind_ip = "127.0.0.1"  # 绑定IP地址 (可选，默认127.0.0.1)
debug = false          # 启用调试模式 (可选，默认false)
```

#### 代理规则部分 (Proxy Rules)
```toml
[[proxies]]
local_port = 8080                    # 本地监听端口 (必需)
remote_host = "target.example.com"   # 远程主机 (必需)
remote_port = 80                     # 远程端口 (必需)
source_ip = "10.0.0.1"               # 源IP地址 (可选)
```

#### 反向代理部分 (Reverse Proxies)
```toml
[[reverse_proxies]]
port = 8080            # 反向代理端口 (必需)
```

### 4.2 配置验证

系统在启动前进行以下验证：
- IP地址格式验证（服务器和代理的source_ip）
- 端口号范围检查（1-65535）
- 端口号重复检查
- 必填字段检查（非空）
- 字符串修剪和规范化
- 空值处理（source_ip = null 时降级为警告）

### 4.3 配置错误处理

**配置文件不存在**:
```
Error: Configuration file 'default.toml' not found
```

**端口冲突**:
```
Error: Port 8080 is already in use
```

**无效的TOML格式**:
```
Error: Invalid TOML format: TOML parse error at line 12, column 17: expected string
Tip: Check for syntax errors like 'null' values (should be omitted), missing quotes, or invalid data types
```

## 5. 系统要求

| 项目 | 要求 |
|-----|-----|
| Rust | 1.70+ |
| 操作系统 | Linux, Windows, macOS |
| 最小内存 | 10MB 可用内存 |
| 网络 | TCP/IP 网络支持 |

## 6. 编译和发布

### 6.1 构建方式

**开发构建**:
```bash
cargo build
```

**发布构建**:
```bash
cargo build --release
```

### 6.2 发布优化

发布版本启用以下优化：
- LTO (Link Time Optimization): 启用
- 编码单元: 1 (最大优化)
- 打包方式: abort (最小化panic)
- 优化级别: 3 (最高)
- 符号剥离: 启用
- 调试断言: 禁用
- 溢出检查: 禁用

### 6.3 包安装

**从Cargo安装**:
```bash
cargo install gsc-fq
```

**从源码构建**:
```bash
git clone https://github.com/putao520/gsc-fq
cd gsc-fq
cargo build --release
```

### 6.4 作为系统服务安装

支持作为systemd服务运行。详见README.md。

## 7. 测试

### 7.1 测试类型

| 类型 | 命令 | 说明 |
|-----|-----|-----|
| 所有测试 | `cargo test` | 运行全部测试 |
| 代理功能测试 | `cargo test proxy_functionality_test` | 代理转发功能 |
| 黑洞功能测试 | `cargo test blackhole_functionality_test` | 黑洞模式功能 |
| 库测试 | `cargo test --lib` | 仅库级单元测试 |
| 基准测试 | `cargo bench` | 性能基准测试 |

### 7.2 测试位置

- 集成测试: `tests/real_e2e_integration_test.rs`, `tests/simple_e2e_test.rs`
- 库单元测试: 各源文件中的 `#[cfg(test)]` 模块

## 8. 代码质量

### 8.1 检查工具

**Lint检查**:
```bash
cargo clippy
```

**代码格式检查**:
```bash
cargo fmt --check
```

**代码格式化**:
```bash
cargo fmt
```

### 8.2 代码规范

- 使用Rust标准库命名规范
- 遵循Clippy所有建议
- 代码必须通过fmt格式化
- 完整的文档注释
- 模块化设计

## 9. 错误处理

### 9.1 错误类型

```
AppError
├── Config(ConfigError)
│   ├── ConfigFileNotFound
│   └── InvalidIpAddress
├── Network(NetworkError)
├── Proxy(ProxyError)
└── Internal { message: String }
```

### 9.2 错误传播

系统使用Result类型进行错误传播：
```rust
pub type Result<T> = std::result::Result<T, AppError>;
```

## 10. 安全考虑

### 10.1 源IP欺骗

- 确保使用指定源IP的权限
- 可能受OS级别限制

### 10.2 防火墙配置

- 需要正确配置防火墙规则
- 允许本地和远程端口的通信

### 10.3 配置文件安全

- 保护配置文件的访问权限
- 避免暴露敏感信息（密码、密钥等）
- 建议使用安全的文件权限（如600）

## 11. 使用场景

1. **网络测试**: 进行网络连接和延迟测试
2. **服务迁移**: 在迁移过程中转发流量
3. **负载均衡**: 简单的流量分配
4. **协议分析**: 拦截和分析通信数据
5. **安全测试**: 进行渗透测试和安全评估

## 12. 系统架构

```
客户端 → GSC-FQ → 目标服务器
         (代理转发)
```

**转发流程**:
1. GSC-FQ监听本地端口
2. 接受客户端连接
3. 建立与远程服务器的连接
4. 双向转发数据
5. 管理连接生命周期

## 13. 性能特性

### 13.1 并发能力

- 支持数千并发连接
- 基于Tokio异步任务
- 单线程高效调度
- CPU亲和性配置

### 13.2 内存优化

- Zero-Copy数据转发
- 高效字节缓冲管理
- 连接池优化
- 内存自动清理

### 13.3 网络优化

- TCP参数自动优化
- Socket级别优化
- 快速数据路径

## 14. 开发工作流

### 14.1 提交要求

- 代码必须通过 `cargo test`
- 代码必须通过 `cargo clippy`
- 代码必须符合 `cargo fmt` 格式
- 添加适当的测试用例
- 更新相关文档

### 14.2 版本管理

- 遵循语义版本控制
- 在CHANGELOG.md中记录变更
- Git标签标记版本

## 15. 部署指南

### 15.1 基本部署

1. 安装gsc-fq或从源码构建
2. 创建配置文件`default.toml`
3. 运行 `gsc-fq`

### 15.2 生产部署

- 使用发布构建 (`--release`)
- 配置为systemd服务
- 设置适当的日志级别
- 监控系统资源使用
- 定期更新到最新版本

## 16. 故障排查

### 16.1 常见问题

| 问题 | 解决方案 |
|-----|--------|
| 配置文件不找不到 | 确保default.toml在当前工作目录 |
| 端口已被使用 | 更改配置的local_port或检查进程占用 |
| 无法连接到远程服务器 | 检查网络连接和防火墙配置 |
| 高CPU/内存使用 | 减少并发连接或优化配置 |

### 16.2 调试

启用调试模式：
```toml
[server]
debug = true
```

## 17. 贡献指南

### 17.1 贡献流程

1. Fork项目
2. 创建功能分支
3. 提交更改
4. 通过所有测试和检查
5. 创建Pull Request

### 17.2 代码审查标准

- 功能完整性和正确性
- 测试覆盖率
- 代码质量和风格
- 文档完整性
- 性能影响分析

## 18. 许可证

项目采用双重许可：
- MIT许可证
- Apache-2.0许可证

详见LICENSE-MIT和LICENSE-APACHE文件。

---

**文档版本**: 1.0  
**最后更新**: 2024年11月  
**作者**: Claude Code AI  
