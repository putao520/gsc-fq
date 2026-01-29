pub mod adaptive_copy;
pub mod adaptive_stream;
pub mod blackhole;
#[cfg(target_os = "linux")]
pub mod splice_optimizer;
pub mod connection_metrics;
pub mod connection_selector;
pub mod handler;
pub mod high_perf;
pub mod quality_aware_connection;
pub mod resilient;
pub mod server;
pub mod stealth_connection_handler;
pub mod stealth_handler;
pub mod zero_copy;

// Explicit exports to avoid ambiguous glob re-exports warning
// Only re-export the most commonly used items
pub use adaptive_copy::adaptive_copy;
pub use adaptive_stream::adaptive_stream_copy;
pub use connection_selector::SelectionStrategy;
pub use handler::ConnectionHandler;
pub use server::{ProxyInstance, ProxyServer, ProxyServerBuilder};
pub use stealth_connection_handler::StealthConnectionHandler;
