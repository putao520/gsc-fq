use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// 连接质量指标
#[derive(Debug, Clone)]
pub struct ConnectionMetrics {
    /// 往返时延 (RTT)
    pub rtt: Option<Duration>,
    
    /// RTT样本 (最近100个)
    pub rtt_samples: VecDeque<Duration>,
    
    /// 丢包率 (0.0 - 1.0)
    pub packet_loss_rate: f64,
    
    /// 发送的包数
    pub packets_sent: u64,
    
    /// 丢失的包数
    pub packets_lost: u64,
    
    /// 成功计数
    pub success_count: u64,
    
    /// 失败计数
    pub failure_count: u64,
    
    /// 带宽 (MB/s)
    pub bandwidth_mbps: f64,
    
    /// 发送字节数
    pub bytes_sent: u64,
    
    /// 接收字节数
    pub bytes_received: u64,
    
    /// 连接创建时间
    pub created_at: Instant,
    
    /// 最后一次健康检查
    pub last_health_check: Instant,
    
    /// 最后一次活动
    pub last_activity: Instant,
}

impl ConnectionMetrics {
    /// 创建新的连接指标
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            rtt: None,
            rtt_samples: VecDeque::with_capacity(100),
            packet_loss_rate: 0.0,
            packets_sent: 0,
            packets_lost: 0,
            success_count: 0,
            failure_count: 0,
            bandwidth_mbps: 0.0,
            bytes_sent: 0,
            bytes_received: 0,
            created_at: now,
            last_health_check: now,
            last_activity: now,
        }
    }
    
    /// 计算质量评分 (0-100)
    pub fn calculate_quality_score(&self) -> u8 {
        // RTT评分 (40%)
        let rtt_score = self.calculate_rtt_score();
        
        // 丢包率评分 (30%)
        let loss_score = self.calculate_loss_score();
        
        // 成功率评分 (20%)
        let success_score = self.calculate_success_score();
        
        // 带宽评分 (10%)
        let bandwidth_score = self.calculate_bandwidth_score();
        
        // 加权平均
        let total_score = (rtt_score * 40.0 + 
                          loss_score * 30.0 + 
                          success_score * 20.0 + 
                          bandwidth_score * 10.0) / 100.0;
        
        total_score.min(100.0).max(0.0) as u8
    }
    
    /// 计算RTT评分
    fn calculate_rtt_score(&self) -> f64 {
        match self.avg_rtt() {
            None => 50.0, // 没有数据，给中等分
            Some(rtt) if rtt < Duration::from_millis(50) => 100.0,
            Some(rtt) if rtt < Duration::from_millis(100) => 90.0,
            Some(rtt) if rtt < Duration::from_millis(200) => 70.0,
            Some(rtt) if rtt < Duration::from_millis(500) => 40.0,
            Some(_) => 10.0,
        }
    }
    
    /// 计算丢包率评分
    fn calculate_loss_score(&self) -> f64 {
        (1.0 - self.packet_loss_rate) * 100.0
    }
    
    /// 计算成功率评分
    fn calculate_success_score(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            return 50.0; // 没有数据，给中等分
        }
        (self.success_count as f64 / total as f64) * 100.0
    }
    
    /// 计算带宽评分
    fn calculate_bandwidth_score(&self) -> f64 {
        // 带宽评分：根据跨洋场景调整
        if self.bandwidth_mbps > 10.0 {
            100.0
        } else if self.bandwidth_mbps > 5.0 {
            80.0
        } else if self.bandwidth_mbps > 1.0 {
            60.0
        } else if self.bandwidth_mbps > 0.5 {
            40.0
        } else {
            20.0
        }
    }
    
    /// 计算平均RTT
    pub fn avg_rtt(&self) -> Option<Duration> {
        if self.rtt_samples.is_empty() {
            return None;
        }
        
        let sum: Duration = self.rtt_samples.iter().sum();
        Some(sum / self.rtt_samples.len() as u32)
    }
    
    /// 添加RTT样本
    pub fn add_rtt_sample(&mut self, rtt: Duration) {
        self.rtt_samples.push_back(rtt);
        if self.rtt_samples.len() > 100 {
            self.rtt_samples.pop_front();
        }
        self.rtt = Some(rtt);
        self.last_health_check = Instant::now();
    }
    
    /// 更新带宽
    pub fn update_bandwidth(&mut self, duration: Duration) {
        let seconds = duration.as_secs_f64();
        if seconds > 0.0 {
            let total_bytes = self.bytes_sent + self.bytes_received;
            self.bandwidth_mbps = (total_bytes as f64 / seconds) / 1_000_000.0;
        }
    }
    
    /// 记录成功
    pub fn record_success(&mut self) {
        self.success_count += 1;
        self.last_activity = Instant::now();
    }
    
    /// 记录失败
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_activity = Instant::now();
    }
    
    /// 更新数据传输统计
    pub fn update_transfer(&mut self, sent: u64, received: u64) {
        self.bytes_sent += sent;
        self.bytes_received += received;
        self.last_activity = Instant::now();
    }
}

impl Default for ConnectionMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_new_metrics() {
        let metrics = ConnectionMetrics::new();
        assert_eq!(metrics.rtt, None);
        assert_eq!(metrics.rtt_samples.len(), 0);
        assert_eq!(metrics.success_count, 0);
        assert_eq!(metrics.failure_count, 0);
    }
    
    #[test]
    fn test_quality_score_no_data() {
        let metrics = ConnectionMetrics::new();
        let score = metrics.calculate_quality_score();
        // 没有RTT数据但有默认带宽和丢包率时应该得62分
        // RTT:50(40%) + Loss:100(30%) + Success:50(20%) + Bandwidth:20(10%) = 62
        assert_eq!(score, 62, "New connection should score 62, got {}", score);
    }
    
    #[test]
    fn test_quality_score_excellent() {
        let mut metrics = ConnectionMetrics::new();
        metrics.add_rtt_sample(Duration::from_millis(30));
        metrics.packet_loss_rate = 0.0;
        metrics.success_count = 100;
        metrics.failure_count = 0;
        metrics.bandwidth_mbps = 15.0;
        
        let score = metrics.calculate_quality_score();
        assert!(score >= 95, "Excellent connection should score >= 95, got {}", score);
    }
    
    #[test]
    fn test_quality_score_poor() {
        let mut metrics = ConnectionMetrics::new();
        metrics.add_rtt_sample(Duration::from_millis(600));
        metrics.packet_loss_rate = 0.15;
        metrics.success_count = 10;
        metrics.failure_count = 90;
        metrics.bandwidth_mbps = 0.3;
        
        let score = metrics.calculate_quality_score();
        assert!(score <= 35, "Poor connection should score <= 35, got {}", score);
    }
    
    #[test]
    fn test_rtt_samples_limit() {
        let mut metrics = ConnectionMetrics::new();
        
        // 添加150个样本
        for i in 0..150 {
            metrics.add_rtt_sample(Duration::from_millis(i));
        }
        
        // 应该只保留最后100个
        assert_eq!(metrics.rtt_samples.len(), 100);
    }
    
    #[test]
    fn test_avg_rtt() {
        let mut metrics = ConnectionMetrics::new();
        
        metrics.add_rtt_sample(Duration::from_millis(100));
        metrics.add_rtt_sample(Duration::from_millis(200));
        metrics.add_rtt_sample(Duration::from_millis(300));
        
        let avg = metrics.avg_rtt().unwrap();
        assert_eq!(avg, Duration::from_millis(200));
    }
    
    #[test]
    fn test_record_success_failure() {
        let mut metrics = ConnectionMetrics::new();
        
        metrics.record_success();
        metrics.record_success();
        metrics.record_failure();
        
        assert_eq!(metrics.success_count, 2);
        assert_eq!(metrics.failure_count, 1);
        
        let score = metrics.calculate_success_score();
        assert!((score - 66.66).abs() < 1.0, "Success score should be ~66.66, got {}", score);
    }
}
