//! GSC-FQ 高性能数据流代理转发CLI工具
//!
//! 这个库提供了一个高性能的TCP代理转发工具，支持：
//! - TOML配置文件
//! - 多网卡数据流向控制
//! - Zero-Copy数据传输
//! - 自动TCP参数优化
//! - 智能内存和连接池管理

pub mod cli;
pub mod config;
pub mod error;
pub mod proxy;
pub mod utils;

// 重新导出主要类型和函数
pub use cli::Args;
pub use config::{ConfigFile, ConfigLoader, ProxySection, ServerSection};
pub use error::{AppError, ConfigError, NetworkError, ProxyError, Result};
