# GSC-FQ 架构文档

## 1. 架构概述

GSC-FQ采用模块化设计，将不同的功能职责分离到独立的模块中，以提高代码的可维护性和可扩展性。

### 1.1 核心设计原则

1. **模块化**: 每个模块独立实现特定功能
2. **异步优先**: 使用Tokio异步运行时
3. **零拷贝**: 高效的数据转发
4. **配置驱动**: 通过TOML配置控制行为
5. **错误恢复**: 优雅的错误处理和资源清理

## 2. 模块架构

### 2.1 Config 模块

**职责**: 配置文件的加载和验证

**主要组件**:
- `loader.rs`: 配置文件加载和解析
- `validator.rs`: 配置内容验证

**关键结构体**:
```rust
pub struct ConfigFile {
    pub server: Option<ServerSection>,
    pub proxies: Vec<ProxySection>,
    pub reverse_proxies: Vec<ReverseProxySection>,
}

pub struct ServerSection {
    pub bind_ip: Option<String>,
    pub debug: Option<bool>,
}

pub struct ProxySection {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub source_ip: Option<String>,
}

pub struct ReverseProxySection {
    pub port: u16,
}
```

**验证规则**:
- IP地址格式验证
- 端口范围检查 (1-65535)
- 端口唯一性检查
- 必填字段检查
- 字符串规范化

### 2.2 Error 模块

**职责**: 统一的错误处理

**错误层级**:
```rust
pub enum AppError {
    Config(ConfigError),
    Network(NetworkError),
    Proxy(ProxyError),
    Internal { message: String },
}

pub enum ConfigError {
    ConfigFileNotFound(String),
    InvalidToml(String),
    InvalidIpAddress(String),
    InvalidPort(String),
    MissingField(String),
}

pub enum NetworkError {
    BindFailed { port: u16, reason: String },
    ConnectionFailed { host: String, reason: String },
    IOError(String),
}

pub enum ProxyError {
    ForwardingFailed { session_id: String, reason: String },
    ProtocolError(String),
}
```

**类型别名**:
```rust
pub type Result<T> = std::result::Result<T, AppError>;
```

### 2.3 Proxy 模块

**职责**: 正向代理的核心实现

**主要组件**:
- `server.rs`: 代理服务器
- `handler.rs`: 连接处理
- `mod.rs`: 模块接口

**核心类型**:

#### ProxyServerBuilder
```rust
pub struct ProxyServerBuilder {
    bind_ip: IpAddr,
    proxies: Vec<ProxyConfig>,
}

impl ProxyServerBuilder {
    pub fn new() -> Self
    pub fn bind_ip(mut self, ip: IpAddr) -> Self
    pub fn add_proxies(mut self, proxies: Vec<ProxySection>) -> Self
    pub fn build(self) -> Result<ProxyServer>
}
```

#### ProxyServer
```rust
pub struct ProxyServer {
    bind_ip: IpAddr,
    listeners: Vec<TcpListener>,
}

impl ProxyServer {
    pub async fn start(&mut self) -> Result<()>
    async fn handle_listener(&self, listener: TcpListener, config: ProxyConfig) -> Result<()>
}
```

**转发流程**:
1. 创建TcpListener监听本地端口
2. 接受客户端连接
3. 建立到远程服务器的连接
4. 启动两个转发任务
   - 客户端→远程服务器
   - 远程服务器→客户端
5. 等待任意一方关闭连接

**数据转发**:
```rust
async fn forward_data(mut from: AsyncRead, mut to: AsyncWrite) -> Result<()> {
    let mut buf = [0u8; 65536];
    loop {
        let n = from.read(&mut buf).await?;
        if n == 0 {
            break; // EOF
        }
        to.write_all(&buf[..n]).await?;
    }
}
```

### 2.4 Reverse Proxy 模块

**职责**: 反向代理的实现

**主要组件**:
- `server.rs`: 反向代理服务器
- `client.rs`: 反向代理客户端
- `protocol.rs`: 通信协议

#### ReverseProxyServer

```rust
pub struct ReverseProxyServer {
    bind_ip: IpAddr,
    control_port: u16,
}

impl ReverseProxyServer {
    pub fn new(bind_ip: IpAddr, control_port: u16) -> Self
    pub async fn start(&mut self) -> Result<()>
    async fn handle_client(&self, socket: TcpStream) -> Result<()>
}
```

**协议**:
- 基于TCP连接
- 使用bincode进行序列化
- 支持多路复用（yamux）

#### ReverseProxyClient

```rust
pub struct ReverseProxyClient {
    server_addr: SocketAddr,
    config: ConfigFile,
}

impl ReverseProxyClient {
    pub fn new(server_addr: SocketAddr, config: ConfigFile) -> Self
    pub async fn start(&mut self) -> Result<()>
}
```

### 2.5 Utils 模块

**职责**: 工具和辅助函数

**子模块**:

#### debug.rs
- 调试模式初始化
- 日志系统配置
- 条件编译的日志宏

#### system.rs
- 系统需求检查
- CPU信息查询
- 内存可用性检查

## 3. 数据流

### 3.1 正向代理数据流

```
┌─────────┐
│ Client  │
└────┬────┘
     │ TCP连接
     ▼
┌──────────────────────────┐
│ GSC-FQ Listen Port 8080  │
└────┬─────────────────────┘
     │ 读取请求
     ▼
┌──────────────────────────┐
│ 建立Remote连接           │
│ remote_host:remote_port  │
└────┬─────────────────────┘
     │ 转发数据
     ▼
┌──────────────────────────┐
│ Remote Server            │
└──────────────────────────┘
```

### 3.2 反向代理数据流

```
┌──────────────────┐
│ ReverseProxy     │
│ Server           │
│ Listen :7000     │
└────┬─────────────┘
     │ 等待客户端
     ▼
┌──────────────────────────┐
│ ReverseProxy             │
│ Client                   │
│ 连接 Server:7000         │
│ 发送代理规则             │
└────┬─────────────────────┘
     │ 建立代理通道
     ▼
┌──────────────────────────┐
│ 多路复用通道 (yamux)     │
│ 支持多个并发代理         │
└────┬─────────────────────┘
     │
     ▼
┌──────────────────────────┐
│ 转发数据到目标服务器     │
└──────────────────────────┘
```

## 4. 并发模型

### 4.1 Tokio任务模型

```
┌─────────────────────────────────────────┐
│ Tokio Runtime                           │
│ (Async Executor)                        │
├─────────────────────────────────────────┤
│ ┌─────────────────┐                     │
│ │ Main Task       │                     │
│ │ - 初始化        │                     │
│ │ - 启动监听      │                     │
│ └────────┬────────┘                     │
│          │                              │
│ ┌────────▼────────┐  ┌────────────────┐ │
│ │ Listener Task1  │  │ Listener Task2 │ │
│ │ Port 8080       │  │ Port 5432      │ │
│ └────────┬────────┘  └────────┬───────┘ │
│          │                    │         │
│ ┌────────▼─────────────────────▼──────┐ │
│ │ Connection Handler Tasks             │ │
│ │ (per accepted connection)            │ │
│ │                                      │ │
│ │ ┌─────────────────────────────────┐ │ │
│ │ │ Forward Task (Client→Remote)    │ │ │
│ │ │ Backward Task (Remote→Client)   │ │ │
│ │ └─────────────────────────────────┘ │ │
│ └──────────────────────────────────────┘ │
└─────────────────────────────────────────┘
```

### 4.2 任务层级

1. **主任务**: 程序启动和初始化
2. **监听任务**: 每个监听端口一个任务
3. **连接处理任务**: 每个连接一对任务（前向和反向转发）
4. **数据转发任务**: 实际的数据复制操作

## 5. 内存管理

### 5.1 缓冲区策略

```rust
const BUFFER_SIZE: usize = 65536; // 64KB缓冲区

// 栈分配缓冲区
let mut buf = [0u8; BUFFER_SIZE];

// 单次读写循环
loop {
    let n = reader.read(&mut buf).await?;
    if n == 0 { break; }
    writer.write_all(&buf[..n]).await?;
}
```

### 5.2 零拷贝优化

- 使用&[u8]避免数据复制
- 直接操作socket缓冲区
- 利用OS级别的sendfile等优化

### 5.3 资源清理

- 自动关闭连接
- Drop trait实现资源回收
- RAII模式确保清理

## 6. 错误恢复

### 6.1 优雅关闭

```rust
// 信号处理
match signal.recv().await {
    Some(sig) => handle_shutdown(sig),
    None => eprintln!("Signal handler closed"),
}

// 资源清理
drop(listener);
drop(active_connections);
```

### 6.2 重试机制

- 连接失败时的重试
- 可配置的超时时间
- 指数退避策略（可选）

## 7. 性能优化

### 7.1 编译优化

```toml
[profile.release]
lto = true              # 链接时优化
codegen-units = 1      # 单编码单元（最大优化）
panic = "abort"         # 直接abort
opt-level = 3           # 最高优化级别
strip = true            # 剥离符号
debug-assertions = false # 禁用调试断言
overflow-checks = false  # 禁用溢出检查
```

### 7.2 运行时优化

- Tokio工作线程数自适应
- CPU亲和性配置
- 动态缓冲区调整

## 8. 配置驱动

### 8.1 配置加载流程

```
┌──────────────────────┐
│ 程序启动             │
└──────────┬───────────┘
           │
┌──────────▼───────────┐
│ 读取default.toml     │
└──────────┬───────────┘
           │
┌──────────▼───────────┐
│ 解析TOML格式         │
└──────────┬───────────┘
           │
┌──────────▼───────────┐
│ 验证配置             │
│ - IP格式             │
│ - 端口范围           │
│ - 必填字段           │
└──────────┬───────────┘
           │
┌──────────▼───────────┐
│ 创建代理配置对象     │
└──────────┬───────────┘
           │
┌──────────▼───────────┐
│ 启动代理服务         │
└──────────────────────┘
```

### 8.2 运行时配置

- 通过[server]部分配置全局行为
- 通过[[proxies]]部分配置单个代理规则
- 支持多套配置文件（不同环境）

## 9. 协议设计

### 9.1 正向代理（TCP转发）

- 无自定义协议
- 直接TCP数据转发
- 保留原始数据完整性

### 9.2 反向代理（自定义协议）

#### 握手阶段
1. Client连接到Server的control_port
2. Client发送代理规则集合（bincode序列化）
3. Server确认并准备建立转发通道

#### 转发阶段
1. 使用yamux建立多路复用通道
2. 每个代理规则对应一个虚拟连接
3. 双向转发数据

#### 断开阶段
1. 无数据时自动关闭
2. 定期心跳检测
3. 异常情况主动关闭

## 10. 扩展性

### 10.1 添加新的工作模式

1. 在main.rs中添加新的命令分支
2. 实现新的模式处理函数
3. 添加相应的配置结构
4. 编写测试用例

### 10.2 添加新的转发规则

1. 在Config模块中定义新的规则结构
2. 在Proxy模块中添加处理逻辑
3. 更新验证规则
4. 更新文档

## 11. 测试架构

### 11.1 单元测试

- 模块级别的测试
- 位于各模块中的#[cfg(test)]块
- 测试配置解析、验证等

### 11.2 集成测试

- 完整功能测试
- 位于tests/目录
- 包括代理功能测试和黑洞检测测试

### 11.3 基准测试

```bash
cargo bench
```

- 使用Criterion库
- HTML报告输出
- 性能回归检测

---

**文档版本**: 1.0  
**最后更新**: 2024年11月  
