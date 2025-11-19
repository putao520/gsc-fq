pub mod blackhole;
pub mod connection_pool;
pub mod connection_metrics;
pub mod connection_selector;
pub mod handler;
pub mod high_perf;
pub mod quality_aware_connection;
pub mod resilient;
pub mod server;
pub mod stealth_connection_handler;
pub mod stealth_handler;
#[cfg(target_os = "linux")]
pub mod zero_copy;

// Explicit exports to avoid ambiguous glob re-exports warning
// Only re-export the most commonly used items
pub use connection_pool::ConnectionPool;
pub use connection_selector::SelectionStrategy;
pub use handler::ConnectionHandler;
pub use server::{ProxyInstance, ProxyServer, ProxyServerBuilder};
pub use stealth_connection_handler::StealthConnectionHandler;
