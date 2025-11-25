//! Comprehensive monitoring and metrics collection for GSC-FQ proxy system
//!
//! This module provides real-time monitoring capabilities including:
//! - Connection metrics and statistics
//! - Performance monitoring and alerts
//! - Resource utilization tracking
//! - Health checks and status reporting
//! - Export to monitoring systems (Prometheus, etc.)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

/// Global monitoring instance
static MONITORING_INSTANCE: std::sync::OnceLock<Arc<RwLock<MonitoringSystem>>> = std::sync::OnceLock::new();

/// Connection monitoring metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConnectionMetrics {
    /// Total connections established
    pub total_connections: u64,
    /// Active connections count
    pub active_connections: u64,
    /// Failed connections count
    pub failed_connections: u64,
    /// Connection establishment rate (connections/second)
    pub connection_rate: f64,
    /// Average connection duration in milliseconds
    pub avg_connection_duration_ms: u64,
    /// Total bytes transferred
    pub total_bytes_transferred: u64,
    /// Total packets transferred
    pub total_packets_transferred: u64,
}

/// Performance monitoring metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// CPU utilization percentage (0-100)
    pub cpu_utilization_percent: f64,
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
    /// Network throughput in MB/s
    pub network_throughput_mbps: f64,
    /// Request processing rate (requests/second)
    pub request_rate: f64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: u64,
    /// Error rate percentage (0-100)
    pub error_rate_percent: f64,
    /// System load average (1, 5, 15 minutes)
    pub load_average: (f64, f64, f64),
}

/// Error monitoring metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ErrorMetrics {
    /// Total error count
    pub total_errors: u64,
    /// Error count by type
    pub errors_by_type: HashMap<String, u64>,
    /// Recent errors (last 100)
    pub recent_errors: Vec<ErrorEvent>,
    /// Error rate trend (errors/second over time)
    pub error_rate_trend: Vec<(std::time::SystemTime, f64)>,
}

/// Error event for tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    pub timestamp: std::time::SystemTime,
    pub error_type: String,
    pub error_message: String,
    pub context: HashMap<String, String>,
}

/// Health check status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// System health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    pub status: HealthStatus,
    pub uptime_seconds: u64,
    pub last_check: std::time::SystemTime,
    pub services: HashMap<String, ServiceHealth>,
}

/// Individual service health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    pub name: String,
    pub status: HealthStatus,
    pub response_time_ms: u64,
    pub last_check: std::time::SystemTime,
    pub details: HashMap<String, String>,
}

/// Comprehensive monitoring system
#[derive(Debug)]
pub struct MonitoringSystem {
    /// Connection metrics
    pub connections: ConnectionMetrics,
    /// Performance metrics
    pub performance: PerformanceMetrics,
    /// Error metrics
    pub errors: ErrorMetrics,
    /// System health
    pub health: SystemHealth,
    /// System start time
    pub start_time: Instant,
    /// Last metrics update
    pub last_update: Instant,
    /// Metrics collection interval
    pub collection_interval: Duration,
}

impl MonitoringSystem {
    /// Create new monitoring system
    pub fn new() -> Self {
        let start_time = Instant::now();
        Self {
            connections: ConnectionMetrics::default(),
            performance: PerformanceMetrics::default(),
            errors: ErrorMetrics::default(),
            health: SystemHealth {
                status: HealthStatus::Healthy,
                uptime_seconds: 0,
                last_check: std::time::SystemTime::now(),
                services: HashMap::new(),
            },
            start_time,
            last_update: Instant::now(),
            collection_interval: Duration::from_secs(5),
        }
    }

    /// Get global monitoring instance
    pub fn global() -> Arc<RwLock<Self>> {
        MONITORING_INSTANCE.get_or_init(|| {
            Arc::new(RwLock::new(Self::new()))
        }).clone()
    }

    /// Record new connection established
    pub async fn record_connection_established(&mut self) {
        self.connections.total_connections += 1;
        self.connections.active_connections += 1;
        self.update_connection_rate().await;
    }

    /// Record connection closed
    pub async fn record_connection_closed(&mut self, duration_ms: u64) {
        if self.connections.active_connections > 0 {
            self.connections.active_connections -= 1;
        }

        // Update average connection duration
        let total_connections = self.connections.total_connections;
        if total_connections > 0 {
            self.connections.avg_connection_duration_ms =
                (self.connections.avg_connection_duration_ms * (total_connections - 1) + duration_ms) / total_connections;
        }
    }

    /// Record failed connection
    pub async fn record_connection_failed(&mut self) {
        self.connections.failed_connections += 1;
        self.update_connection_rate().await;
    }

    /// Record data transfer
    pub async fn record_data_transfer(&mut self, bytes: u64, packets: u64) {
        self.connections.total_bytes_transferred += bytes;
        self.connections.total_packets_transferred += packets;
        self.update_network_throughput().await;
    }

    /// Record error event
    pub async fn record_error(&mut self, error_type: String, message: String, context: HashMap<String, String>) {
        self.errors.total_errors += 1;
        *self.errors.errors_by_type.entry(error_type.clone()).or_insert(0) += 1;

        let error_event = ErrorEvent {
            timestamp: std::time::SystemTime::now(),
            error_type,
            error_message: message,
            context,
        };

        self.errors.recent_errors.push(error_event);

        // Keep only last 100 errors
        if self.errors.recent_errors.len() > 100 {
            self.errors.recent_errors.remove(0);
        }

        self.update_error_rate().await;
    }

    /// Update performance metrics
    pub async fn update_performance_metrics(&mut self) {
        self.performance.cpu_utilization_percent = self.get_cpu_usage().await;
        self.performance.memory_usage_bytes = self.get_memory_usage().await;
        self.performance.request_rate = self.calculate_request_rate().await;
        self.performance.error_rate_percent = self.calculate_error_rate().await;

        // Update system health based on metrics
        self.update_health_status().await;

        self.last_update = Instant::now();
    }

    /// Get current system status summary
    pub async fn get_status_summary(&self) -> serde_json::Value {
        let uptime = self.start_time.elapsed().as_secs();

        serde_json::json!({
            "status": self.health.status,
            "uptime_seconds": uptime,
            "connections": {
                "total": self.connections.total_connections,
                "active": self.connections.active_connections,
                "failed": self.connections.failed_connections,
                "rate": self.connections.connection_rate
            },
            "performance": {
                "cpu_percent": self.performance.cpu_utilization_percent,
                "memory_mb": self.performance.memory_usage_bytes / (1024 * 1024),
                "throughput_mbps": self.performance.network_throughput_mbps,
                "request_rate": self.performance.request_rate,
                "avg_response_time_ms": self.performance.avg_response_time_ms,
                "error_rate_percent": self.performance.error_rate_percent
            },
            "errors": {
                "total": self.errors.total_errors,
                "recent_count": self.errors.recent_errors.len()
            }
        })
    }

    /// Export metrics in Prometheus format
    pub async fn export_prometheus_metrics(&self) -> String {
        let mut output = String::new();

        // Connection metrics
        output.push_str(&format!(
            "# HELP gsc_fq_connections_total Total number of connections\n\
             # TYPE gsc_fq_connections_total counter\n\
             gsc_fq_connections_total {}\n",
            self.connections.total_connections
        ));

        output.push_str(&format!(
            "# HELP gsc_fq_connections_active Current active connections\n\
             # TYPE gsc_fq_connections_active gauge\n\
             gsc_fq_connections_active {}\n",
            self.connections.active_connections
        ));

        output.push_str(&format!(
            "# HELP gsc_fq_bytes_transferred_total Total bytes transferred\n\
             # TYPE gsc_fq_bytes_transferred_total counter\n\
             gsc_fq_bytes_transferred_total {}\n",
            self.connections.total_bytes_transferred
        ));

        // Performance metrics
        output.push_str(&format!(
            "# HELP gsc_fq_cpu_utilization_percent CPU utilization percentage\n\
             # TYPE gsc_fq_cpu_utilization_percent gauge\n\
             gsc_fq_cpu_utilization_percent {}\n",
            self.performance.cpu_utilization_percent
        ));

        output.push_str(&format!(
            "# HELP gsc_fq_memory_usage_bytes Memory usage in bytes\n\
             # TYPE gsc_fq_memory_usage_bytes gauge\n\
             gsc_fq_memory_usage_bytes {}\n",
            self.performance.memory_usage_bytes
        ));

        output.push_str(&format!(
            "# HELP gsc_fq_network_throughput_mbps Network throughput in MB/s\n\
             # TYPE gsc_fq_network_throughput_mbps gauge\n\
             gsc_fq_network_throughput_mbps {}\n",
            self.performance.network_throughput_mbps
        ));

        output.push_str(&format!(
            "# HELP gsc_fq_errors_total Total number of errors\n\
             # TYPE gsc_fq_errors_total counter\n\
             gsc_fq_errors_total {}\n",
            self.errors.total_errors
        ));

        output
    }

    // Private helper methods

    async fn update_connection_rate(&mut self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.connections.connection_rate = self.connections.total_connections as f64 / elapsed;
        }
    }

    async fn update_network_throughput(&mut self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            let bytes_per_second = self.connections.total_bytes_transferred as f64 / elapsed;
            self.performance.network_throughput_mbps = (bytes_per_second * 8.0) / (1024.0 * 1024.0);
        }
    }

    async fn update_error_rate(&mut self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            let errors_per_second = self.errors.total_errors as f64 / elapsed;
            self.errors.error_rate_trend.push((std::time::SystemTime::now(), errors_per_second));

            // Keep only last 100 data points
            if self.errors.error_rate_trend.len() > 100 {
                self.errors.error_rate_trend.remove(0);
            }
        }
    }

    async fn get_cpu_usage(&self) -> f64 {
        // Simplified CPU usage calculation
        // In a real implementation, you would use platform-specific APIs
        // This is a placeholder - real implementation would use sysinfo crate or similar
        rand::random::<f64>() * 80.0 + 10.0 // 10-90% random usage for demo
    }

    async fn get_memory_usage(&self) -> u64 {
        // Simplified memory usage calculation
        // In a real implementation, you would use platform-specific APIs
        512 * 1024 * 1024 // 512MB placeholder
    }

    async fn calculate_request_rate(&self) -> f64 {
        // Calculate request rate based on packets and connections
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.connections.total_packets_transferred as f64 / elapsed
        } else {
            0.0
        }
    }

    async fn calculate_error_rate(&self) -> f64 {
        let total_requests = self.connections.total_connections;
        if total_requests > 0 {
            (self.errors.total_errors as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        }
    }

    async fn update_health_status(&mut self) {
        let uptime = self.start_time.elapsed().as_secs();
        self.health.uptime_seconds = uptime;
        self.health.last_check = std::time::SystemTime::now();

        // Determine health status based on metrics
        self.health.status = if self.performance.cpu_utilization_percent > 90.0
            || self.performance.error_rate_percent > 10.0 {
            HealthStatus::Unhealthy
        } else if self.performance.cpu_utilization_percent > 70.0
            || self.performance.error_rate_percent > 5.0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };
    }
}

impl Default for MonitoringSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience functions for global monitoring access

/// Record connection established
pub async fn record_connection_established() {
    let monitoring = MonitoringSystem::global();
    {
        let mut m = monitoring.write().await;
        m.record_connection_established().await;
    }
}

/// Record connection closed
pub async fn record_connection_closed(duration_ms: u64) {
    let monitoring = MonitoringSystem::global();
    {
        let mut m = monitoring.write().await;
        m.record_connection_closed(duration_ms).await;
    }
}

/// Record failed connection
pub async fn record_connection_failed() {
    let monitoring = MonitoringSystem::global();
    {
        let mut m = monitoring.write().await;
        m.record_connection_failed().await;
    }
}

/// Record data transfer
pub async fn record_data_transfer(bytes: u64, packets: u64) {
    let monitoring = MonitoringSystem::global();
    {
        let mut m = monitoring.write().await;
        m.record_data_transfer(bytes, packets).await;
    }
}

/// Record error
pub async fn record_error(error_type: String, message: String, context: HashMap<String, String>) {
    let monitoring = MonitoringSystem::global();
    {
        let mut m = monitoring.write().await;
        m.record_error(error_type, message, context).await;
    }
}

/// Get status summary
pub async fn get_status_summary() -> serde_json::Value {
    let monitoring = MonitoringSystem::global();
    let m = monitoring.read().await;
    m.get_status_summary().await
}

/// Export Prometheus metrics
pub async fn export_prometheus_metrics() -> String {
    let monitoring = MonitoringSystem::global();
    let m = monitoring.read().await;
    m.export_prometheus_metrics().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_monitoring_system_creation() {
        let monitoring = MonitoringSystem::new();
        assert_eq!(monitoring.connections.total_connections, 0);
        assert_eq!(monitoring.connections.active_connections, 0);
    }

    #[tokio::test]
    async fn test_connection_tracking() {
        let mut monitoring = MonitoringSystem::new();

        // Test connection establishment
        monitoring.record_connection_established().await;
        assert_eq!(monitoring.connections.total_connections, 1);
        assert_eq!(monitoring.connections.active_connections, 1);

        // Test connection closure
        monitoring.record_connection_closed(1000).await;
        assert_eq!(monitoring.connections.active_connections, 0);
    }

    #[tokio::test]
    async fn test_error_tracking() {
        let mut monitoring = MonitoringSystem::new();

        let mut context = HashMap::new();
        context.insert("test_key".to_string(), "test_value".to_string());

        monitoring.record_error(
            "TestError".to_string(),
            "Test error message".to_string(),
            context,
        ).await;

        assert_eq!(monitoring.errors.total_errors, 1);
        assert_eq!(monitoring.errors.recent_errors.len(), 1);
        assert_eq!(monitoring.errors.errors_by_type.get("TestError"), Some(&1));
    }

    #[tokio::test]
    async fn test_global_monitoring() {
        // Use a fresh monitoring instance for testing
        record_connection_established().await;
        record_connection_closed(100).await; // Close the connection for clean test

        let summary = get_status_summary().await;

        // The global instance may have existing state, so just verify structure
        assert!(summary.get("connections").is_some());
        assert!(summary.get("performance").is_some());
        assert!(summary.get("errors").is_some());
    }
}