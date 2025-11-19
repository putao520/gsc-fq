# GSC-FQ API 文档

## 1. 公共API

GSC-FQ提供以下公共API供第三方集成使用。

### 1.1 库导入

```rust
use gsc_fq::{
    config::{ConfigFile, ConfigLoader, ProxySection, ServerSection},
    error::{AppError, ConfigError, NetworkError, ProxyError, Result},
    proxy::ProxyServerBuilder,
    reverse_proxy::{ReverseProxyServer, ReverseProxyClient},
};
```

## 2. Config 模块

### 2.1 ConfigLoader

配置文件加载器，用于从TOML文件加载配置。

#### 方法

```rust
impl ConfigLoader {
    /// 从指定文件加载配置
    pub fn load_from_file(path: &str) -> Result<ConfigFile>
    
    /// 从字符串加载配置
    pub fn load_from_string(content: &str) -> Result<ConfigFile>
}
```

#### 示例

```rust
use gsc_fq::config::ConfigLoader;

// 从文件加载
let config = ConfigLoader::load_from_file("default.toml")?;

// 从字符串加载
let toml_content = r#"
[server]
bind_ip = "127.0.0.1"
debug = false

[[proxies]]
local_port = 8080
remote_host = "example.com"
remote_port = 80
"#;
let config = ConfigLoader::load_from_string(toml_content)?;
```

### 2.2 ConfigFile

顶级配置结构体。

```rust
pub struct ConfigFile {
    pub server: Option<ServerSection>,
    pub proxies: Vec<ProxySection>,
    pub reverse_proxies: Vec<ReverseProxySection>,
}
```

#### 字段说明

| 字段 | 类型 | 说明 |
|-----|-----|-----|
| server | Option<ServerSection> | 服务器全局配置 |
| proxies | Vec<ProxySection> | 正向代理规则列表 |
| reverse_proxies | Vec<ReverseProxySection> | 反向代理规则列表 |

### 2.3 ServerSection

服务器全局配置。

```rust
pub struct ServerSection {
    pub bind_ip: Option<String>,
    pub debug: Option<bool>,
}
```

#### 字段说明

| 字段 | 类型 | 说明 | 默认值 |
|-----|-----|-----|--------|
| bind_ip | Option<String> | 绑定IP地址 | "127.0.0.1" |
| debug | Option<bool> | 启用调试模式 | false |

### 2.4 ProxySection

单个代理规则。

```rust
pub struct ProxySection {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub source_ip: Option<String>,
}
```

#### 字段说明

| 字段 | 类型 | 必需 | 说明 |
|-----|-----|------|-----|
| local_port | u16 | ✓ | 本地监听端口 |
| remote_host | String | ✓ | 远程主机地址 |
| remote_port | u16 | ✓ | 远程端口 |
| source_ip | Option<String> | | 源IP地址（可选） |

### 2.5 ReverseProxySection

反向代理规则。

```rust
pub struct ReverseProxySection {
    pub port: u16,
}
```

#### 字段说明

| 字段 | 类型 | 必需 | 说明 |
|-----|-----|------|-----|
| port | u16 | ✓ | 反向代理端口 |

## 3. Error 模块

### 3.1 AppError

应用程序错误枚举。

```rust
pub enum AppError {
    Config(ConfigError),
    Network(NetworkError),
    Proxy(ProxyError),
    Internal { message: String },
}
```

### 3.2 ConfigError

配置错误。

```rust
pub enum ConfigError {
    ConfigFileNotFound(String),
    InvalidToml(String),
    InvalidIpAddress(String),
    InvalidPort(String),
    MissingField(String),
}
```

#### 示例

```rust
use gsc_fq::error::{AppError, ConfigError};

match error {
    AppError::Config(ConfigError::ConfigFileNotFound(path)) => {
        eprintln!("Config file not found: {}", path);
    }
    AppError::Config(ConfigError::InvalidToml(msg)) => {
        eprintln!("Invalid TOML: {}", msg);
    }
    _ => {}
}
```

### 3.3 NetworkError

网络错误。

```rust
pub enum NetworkError {
    BindFailed { port: u16, reason: String },
    ConnectionFailed { host: String, reason: String },
    IOError(String),
}
```

### 3.4 ProxyError

代理错误。

```rust
pub enum ProxyError {
    ForwardingFailed { session_id: String, reason: String },
    ProtocolError(String),
}
```

### 3.5 Result 类型别名

```rust
pub type Result<T> = std::result::Result<T, AppError>;
```

## 4. Proxy 模块

### 4.1 ProxyServerBuilder

代理服务器构建器，使用Builder模式。

#### 方法

```rust
impl ProxyServerBuilder {
    /// 创建新的构建器
    pub fn new() -> Self
    
    /// 设置绑定IP地址
    pub fn bind_ip(mut self, ip: IpAddr) -> Self
    
    /// 添加代理规则
    pub fn add_proxies(mut self, proxies: Vec<ProxySection>) -> Self
    
    /// 构建ProxyServer
    pub fn build(self) -> Result<ProxyServer>
}
```

#### 示例

```rust
use std::net::IpAddr;
use gsc_fq::proxy::ProxyServerBuilder;
use gsc_fq::config::ProxySection;

let proxy_rule = ProxySection {
    local_port: 8080,
    remote_host: "example.com".to_string(),
    remote_port: 80,
    source_ip: None,
};

let mut server = ProxyServerBuilder::new()
    .bind_ip("127.0.0.1".parse()?)
    .add_proxies(vec![proxy_rule])
    .build()?;

server.start().await?;
```

### 4.2 ProxyServer

代理服务器。

#### 方法

```rust
impl ProxyServer {
    /// 启动代理服务器
    pub async fn start(&mut self) -> Result<()>
}
```

#### 示例

```rust
use gsc_fq::proxy::ProxyServerBuilder;

#[tokio::main]
async fn main() -> gsc_fq::error::Result<()> {
    let mut server = ProxyServerBuilder::new()
        .bind_ip("0.0.0.0".parse()?)
        .build()?;
    
    server.start().await
}
```

## 5. ReverseProxy 模块

### 5.1 ReverseProxyServer

反向代理服务器。

#### 构造函数

```rust
impl ReverseProxyServer {
    /// 创建新的反向代理服务器
    pub fn new(bind_ip: IpAddr, control_port: u16) -> Self
}
```

#### 方法

```rust
impl ReverseProxyServer {
    /// 启动反向代理服务器
    pub async fn start(&mut self) -> Result<()>
}
```

#### 示例

```rust
use std::net::IpAddr;
use gsc_fq::reverse_proxy::ReverseProxyServer;

#[tokio::main]
async fn main() -> gsc_fq::error::Result<()> {
    let bind_ip: IpAddr = "0.0.0.0".parse()?;
    let mut server = ReverseProxyServer::new(bind_ip, 7000);
    server.start().await
}
```

### 5.2 ReverseProxyClient

反向代理客户端。

#### 构造函数

```rust
impl ReverseProxyClient {
    /// 创建新的反向代理客户端
    pub fn new(server_addr: SocketAddr, config: ConfigFile) -> Self
}
```

#### 方法

```rust
impl ReverseProxyClient {
    /// 启动反向代理客户端
    pub async fn start(&mut self) -> Result<()>
}
```

#### 示例

```rust
use std::net::SocketAddr;
use gsc_fq::config::ConfigLoader;
use gsc_fq::reverse_proxy::ReverseProxyClient;

#[tokio::main]
async fn main() -> gsc_fq::error::Result<()> {
    let config = ConfigLoader::load_from_file("config_test.toml")?;
    let server_addr: SocketAddr = "127.0.0.1:7000".parse()?;
    
    let mut client = ReverseProxyClient::new(server_addr, config);
    client.start().await
}
```

## 6. Utils 模块

### 6.1 Debug 系统

```rust
pub mod debug {
    /// 初始化调试系统
    pub fn init_debug(enabled: bool)
    
    /// 调试日志宏（条件编译）
    #[macro_export]
    macro_rules! debug_log {
        ($($arg:tt)*) => { ... }
    }
}
```

#### 示例

```rust
use gsc_fq::utils::debug::init_debug;

fn main() {
    init_debug(true);
    debug_log!("Debug message");
}
```

### 6.2 System 检查

```rust
pub mod system {
    /// 检查系统需求
    pub fn check_system_requirements() -> Result<()>
}
```

#### 示例

```rust
use gsc_fq::utils::system::check_system_requirements;

#[tokio::main]
async fn main() -> gsc_fq::error::Result<()> {
    check_system_requirements()?;
    // 继续初始化...
    Ok(())
}
```

## 7. 完整使用示例

### 7.1 正向代理示例

```rust
use std::net::IpAddr;
use gsc_fq::config::ConfigLoader;
use gsc_fq::proxy::ProxyServerBuilder;

#[tokio::main]
async fn main() -> gsc_fq::error::Result<()> {
    // 加载配置
    let config = ConfigLoader::load_from_file("default.toml")?;
    
    // 获取绑定IP
    let bind_ip: IpAddr = config.server
        .as_ref()
        .and_then(|s| s.bind_ip.as_ref())
        .and_then(|ip| ip.parse().ok())
        .unwrap_or_else(|| "127.0.0.1".parse().unwrap());
    
    // 初始化调试
    let debug = config.server
        .as_ref()
        .and_then(|s| s.debug)
        .unwrap_or(false);
    gsc_fq::utils::debug::init_debug(debug);
    
    // 构建并启动代理服务器
    let mut server = ProxyServerBuilder::new()
        .bind_ip(bind_ip)
        .add_proxies(config.proxies)
        .build()?;
    
    server.start().await?;
    Ok(())
}
```

### 7.2 反向代理示例

```rust
use std::net::{IpAddr, SocketAddr};
use gsc_fq::config::ConfigLoader;
use gsc_fq::reverse_proxy::{ReverseProxyServer, ReverseProxyClient};

#[tokio::main]
async fn main() -> gsc_fq::error::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 3 {
        eprintln!("Usage: {} [s|c] <arg>", args[0]);
        std::process::exit(1);
    }
    
    match args[1].as_str() {
        "s" => {
            // 服务器模式
            let port: u16 = args[2].parse()?;
            let mut server = ReverseProxyServer::new("0.0.0.0".parse()?, port);
            server.start().await
        }
        "c" => {
            // 客户端模式
            let server_addr: SocketAddr = args[2].parse()?;
            let config = ConfigLoader::load_from_file("config_test.toml")?;
            let mut client = ReverseProxyClient::new(server_addr, config);
            client.start().await
        }
        _ => Err(gsc_fq::error::AppError::Internal {
            message: "Invalid mode".to_string(),
        }),
    }
}
```

### 7.3 错误处理示例

```rust
use gsc_fq::config::ConfigLoader;
use gsc_fq::error::{AppError, ConfigError};

#[tokio::main]
async fn main() {
    match ConfigLoader::load_from_file("default.toml") {
        Ok(config) => println!("Config loaded successfully"),
        Err(AppError::Config(ConfigError::ConfigFileNotFound(path))) => {
            eprintln!("Config file not found: {}", path);
        }
        Err(AppError::Config(ConfigError::InvalidToml(msg))) => {
            eprintln!("Invalid TOML format: {}", msg);
        }
        Err(e) => eprintln!("Error: {:?}", e),
    }
}
```

## 8. 使用建议

### 8.1 最佳实践

1. **配置验证**: 始终检查加载配置是否成功
2. **错误处理**: 使用`?`操作符或`match`进行错误处理
3. **资源清理**: 使用RAII确保资源正确释放
4. **日志输出**: 启用调试模式进行排查

### 8.2 性能建议

1. **缓冲区大小**: 使用64KB缓冲区以获得最佳性能
2. **连接管理**: 通过配置限制并发连接数
3. **CPU亲和性**: 在多核系统上配置工作线程数

### 8.3 安全建议

1. **配置文件权限**: 使用适当的文件权限保护配置
2. **源IP验证**: 确保有权使用指定的源IP
3. **防火墙配置**: 适当配置防火墙规则

---

**API版本**: 1.0  
**最后更新**: 2024年11月  
