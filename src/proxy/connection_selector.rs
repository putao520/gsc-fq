use super::quality_aware_connection::QualityAwareConnection;
use rand::Rng;

/// 连接选择策略  
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionStrategy {
    /// 选择质量最好的连接
    BestQuality,
    
    /// 加权随机 (质量高的连接被选中概率更大)
    WeightedRandom,
    
    /// 轮询
    RoundRobin,
    
    /// 最少连接数 (预留，用于多连接池场景)
    LeastConnections,
}

impl Default for SelectionStrategy {
    fn default() -> Self {
        SelectionStrategy::BestQuality
    }
}

impl SelectionStrategy {
    /// 从字符串解析策略
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "best_quality" | "best" => Some(SelectionStrategy::BestQuality),
            "weighted_random" | "weighted" => Some(SelectionStrategy::WeightedRandom),
            "round_robin" | "roundrobin" => Some(SelectionStrategy::RoundRobin),
            "least_connections" | "least" => Some(SelectionStrategy::LeastConnections),
            _ => None,
        }
    }
}

/// 连接选择器
pub struct ConnectionSelector;

impl ConnectionSelector {
    /// 从连接池中选择最优连接
    pub async fn select(
        pool: &mut Vec<QualityAwareConnection>,
        strategy: SelectionStrategy,
        round_robin_index: &mut usize,
    ) -> Option<QualityAwareConnection> {
        if pool.is_empty() {
            return None;
        }
        
        match strategy {
            SelectionStrategy::BestQuality => {
                Self::select_best_quality(pool).await
            }
            SelectionStrategy::WeightedRandom => {
                Self::select_weighted_random(pool).await
            }
            SelectionStrategy::RoundRobin => {
                Self::select_round_robin(pool, round_robin_index)
            }
            SelectionStrategy::LeastConnections => {
                // 简化实现：退化为BestQuality
                Self::select_best_quality(pool).await
            }
        }
    }
    
    /// 选择质量最好的连接
    async fn select_best_quality(
        pool: &mut Vec<QualityAwareConnection>
    ) -> Option<QualityAwareConnection> {
        if pool.is_empty() {
            return None;
        }
        
        // 计算所有连接的质量评分
        let mut best_index = 0;
        let mut best_score = pool[0].quality_score().await;
        
        for (idx, conn) in pool.iter().enumerate().skip(1) {
            let score = conn.quality_score().await;
            if score > best_score {
                best_score = score;
                best_index = idx;
            }
        }
        
        // 移除并返回最高分连接
        Some(pool.swap_remove(best_index))
    }
    
    /// 加权随机选择 (质量高的连接被选中概率更大)
    async fn select_weighted_random(
        pool: &mut Vec<QualityAwareConnection>
    ) -> Option<QualityAwareConnection> {
        if pool.is_empty() {
            return None;
        }
        
        // 计算所有连接的质量评分
        let mut scores: Vec<u8> = Vec::with_capacity(pool.len());
        for conn in pool.iter() {
            scores.push(conn.quality_score().await);
        }
        
        // 计算总权重
        let total_weight: u32 = scores.iter().map(|&s| s as u32).sum();
        
        if total_weight == 0 {
            // 所有连接评分都是0，随机选择
            let index = rand::rng().random_range(0..pool.len());
            return Some(pool.swap_remove(index));
        }

        // 加权随机选择
        let mut rng = rand::rng();
        let mut random_weight = rng.random_range(0..total_weight);
        
        for (idx, score) in scores.iter().enumerate() {
            if random_weight < *score as u32 {
                return Some(pool.swap_remove(idx));
            }
            random_weight -= *score as u32;
        }
        
        // 兜底：返回最后一个
        Some(pool.swap_remove(pool.len() - 1))
    }
    
    /// 轮询选择
    fn select_round_robin(
        pool: &mut Vec<QualityAwareConnection>,
        round_robin_index: &mut usize,
    ) -> Option<QualityAwareConnection> {
        if pool.is_empty() {
            return None;
        }
        
        let index = *round_robin_index % pool.len();
        *round_robin_index = (*round_robin_index + 1) % pool.len();
        Some(pool.swap_remove(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::connection_metrics::ConnectionMetrics;
    use std::time::Duration;
    
    async fn create_test_connection_with_score(score: u8) -> QualityAwareConnection {
        use tokio::net::TcpListener;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        // 创建一个测试连接
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();

        let mut metrics = ConnectionMetrics::new();

        // 根据目标分数设置指标
        if score >= 90 {
            metrics.add_rtt_sample(Duration::from_millis(30));
            metrics.success_count = 100;
            metrics.failure_count = 0;
            metrics.bandwidth_mbps = 15.0;
        } else if score >= 70 {
            metrics.add_rtt_sample(Duration::from_millis(100));
            metrics.success_count = 80;
            metrics.failure_count = 20;
            metrics.bandwidth_mbps = 8.0;
        } else {
            metrics.add_rtt_sample(Duration::from_millis(500));
            metrics.success_count = 20;
            metrics.failure_count = 80;
            metrics.bandwidth_mbps = 0.5;
        }

        QualityAwareConnection::new_with_metrics(stream, Arc::new(Mutex::new(metrics)), uuid::Uuid::new_v4().to_string())
    }
    
    #[test]
    fn test_strategy_from_str() {
        assert_eq!(SelectionStrategy::from_str("best_quality"), Some(SelectionStrategy::BestQuality));
        assert_eq!(SelectionStrategy::from_str("WEIGHTED_RANDOM"), Some(SelectionStrategy::WeightedRandom));
        assert_eq!(SelectionStrategy::from_str("roundrobin"), Some(SelectionStrategy::RoundRobin));
        assert_eq!(SelectionStrategy::from_str("invalid"), None);
    }
    
    #[tokio::test]
    async fn test_select_best_quality() {
        let mut pool = vec![
            create_test_connection_with_score(50).await,
            create_test_connection_with_score(90).await,
            create_test_connection_with_score(70).await,
        ];
        
        let selected = ConnectionSelector::select_best_quality(&mut pool).await.unwrap();
        let score = selected.quality_score().await;
        
        // 应该选择评分最高的连接
        assert!(score >= 85, "Should select connection with score ~90, got {}", score);
        assert_eq!(pool.len(), 2); // 池中应该剩余2个连接
    }
    
    #[tokio::test]
    async fn test_select_round_robin() {
        let mut pool = vec![
            create_test_connection_with_score(50).await,
            create_test_connection_with_score(60).await,
            create_test_connection_with_score(70).await,
        ];
        
        let mut index = 0;
        
        // 第一次选择应该是索引0
        let _ = ConnectionSelector::select_round_robin(&mut pool, &mut index);
        assert_eq!(pool.len(), 2);
        assert_eq!(index, 1);
        
        // 第二次选择应该是索引1 (现在pool只有2个元素)
        let _ = ConnectionSelector::select_round_robin(&mut pool, &mut index);
        assert_eq!(pool.len(), 1);
        assert_eq!(index, 0); // 应该回到0
    }
}
