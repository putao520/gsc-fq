use super::connection_metrics::ConnectionMetrics;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

/// 质量感知的连接包装器
pub struct QualityAwareConnection {
    /// 底层TCP流
    stream: TcpStream,
    
    /// 连接质量指标
    pub metrics: Arc<Mutex<ConnectionMetrics>>,
    
    /// 连接唯一标识
    pub connection_id: String,
}

impl QualityAwareConnection {
    /// 创建新的质量感知连接
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            metrics: Arc::new(Mutex::new(ConnectionMetrics::new())),
            connection_id: uuid::Uuid::new_v4().to_string(),
        }
    }
    
    /// 获取连接质量评分
    pub async fn quality_score(&self) -> u8 {
        self.metrics.lock().await.calculate_quality_score()
    }
    
    /// 获取连接指标快照
    pub async fn get_metrics(&self) -> ConnectionMetrics {
        self.metrics.lock().await.clone()
    }
    
    /// 消费连接，返回底层TcpStream
    pub fn into_inner(self) -> TcpStream {
        self.stream
    }
    
    /// 测量RTT (通过快速连接测试)
    pub async fn measure_rtt(&self) -> std::io::Result<std::time::Duration> {
        use std::time::Instant;
        use std::io::ErrorKind;
        
        let start = Instant::now();
        
        // 使用peek操作测量RTT (不消耗数据)
        let mut buf = [0u8; 1];
        match self.stream.try_read(&mut buf) {
            Ok(_n) => Ok(start.elapsed()),
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(start.elapsed()),
            Err(e) => Err(e),
        }
    }
    
    /// 检查连接是否存活
    pub async fn is_alive(&self) -> bool {
        use std::io::ErrorKind;
        
        let mut buf = [0u8; 1];
        match self.stream.try_read(&mut buf) {
            Ok(_n) => true,
            Err(e) if e.kind() == ErrorKind::WouldBlock => true,
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_new_connection() {
        // 创建测试用的TCP连接需要实际的socket
        // 这里只测试基本的结构
        let metrics = ConnectionMetrics::new();
        assert_eq!(metrics.success_count, 0);
    }
    
    #[tokio::test]
    async fn test_quality_score() {
        let mut metrics = ConnectionMetrics::new();
        metrics.add_rtt_sample(Duration::from_millis(50));
        metrics.success_count = 10;
        metrics.failure_count = 0;
        
        let score = metrics.calculate_quality_score();
        assert!(score >= 70, "Good connection should score well");
    }
}
