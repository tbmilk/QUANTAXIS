//! 执行层（Phase 2）
//!
//! - `BrokerAdapter`：经纪商接口抽象（Trait + 模拟实现）
//! - `OrderRouter`：根据市场类型将订单路由到正确经纪商

use crate::qarisk::context::{Direction, Offset, OrderSnapshot};
use crate::qarisk::market::MarketType;

// ─── 订单状态 ─────────────────────────────────────────────────────────────────

/// 成交回报
#[derive(Debug, Clone)]
pub struct TradeReport {
    pub order_id: String,
    pub instrument_id: String,
    pub direction: Direction,
    pub offset: Offset,
    pub price: f64,
    pub volume: i64,
    pub commission: f64,
    pub timestamp_ms: i64,
}

/// 订单状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    PartialFilled,
    Filled,
    Cancelled,
    Rejected,
}

/// 订单确认
#[derive(Debug, Clone)]
pub struct OrderAck {
    pub order_id: String,
    pub status: OrderStatus,
    pub message: String,
    pub timestamp_ms: i64,
}

impl OrderAck {
    pub fn rejected(order_id: &str, reason: &str) -> Self {
        Self {
            order_id: order_id.to_string(),
            status: OrderStatus::Rejected,
            message: reason.to_string(),
            timestamp_ms: 0,
        }
    }

    pub fn accepted(order_id: &str) -> Self {
        Self {
            order_id: order_id.to_string(),
            status: OrderStatus::Pending,
            message: "已接受".to_string(),
            timestamp_ms: 0,
        }
    }
}

// ─── BrokerAdapter Trait ──────────────────────────────────────────────────────

/// 经纪商接口（Strategy Pattern）
pub trait BrokerAdapter: Send + Sync {
    /// 经纪商名称
    fn name(&self) -> &str;

    /// 支持的市场类型
    fn supported_markets(&self) -> &[MarketType];

    /// 是否支持该市场
    fn supports(&self, market: MarketType) -> bool {
        self.supported_markets().contains(&market)
    }

    /// 提交订单（异步在 Trait 中用 Box<dyn Future> 太复杂，这里用同步接口）
    fn submit_order(&self, order: &OrderSnapshot) -> OrderAck;

    /// 撤单
    fn cancel_order(&self, order_id: &str) -> Result<(), String>;

    /// 查询持仓（简化版：返回净持仓量）
    fn query_position(&self, instrument_id: &str) -> i64;
}

// ─── 模拟经纪商 ───────────────────────────────────────────────────────────────

/// 回测 / 测试用模拟经纪商（总是接受订单）
pub struct MockBroker {
    name: String,
    markets: Vec<MarketType>,
}

impl MockBroker {
    pub fn new(name: &str, markets: Vec<MarketType>) -> Self {
        Self { name: name.to_string(), markets }
    }

    /// 适用全市场的通用模拟经纪商
    pub fn universal() -> Self {
        Self::new("MockBroker", vec![
            MarketType::CN,
            MarketType::CNFutures,
            MarketType::HK,
            MarketType::US,
            MarketType::Crypto,
        ])
    }
}

impl BrokerAdapter for MockBroker {
    fn name(&self) -> &str { &self.name }
    fn supported_markets(&self) -> &[MarketType] { &self.markets }

    fn submit_order(&self, order: &OrderSnapshot) -> OrderAck {
        OrderAck {
            order_id: order.order_id.clone(),
            status: OrderStatus::Pending,
            message: format!("[{}] 订单已提交", self.name),
            timestamp_ms: 0,
        }
    }

    fn cancel_order(&self, _order_id: &str) -> Result<(), String> {
        Ok(())
    }

    fn query_position(&self, _instrument_id: &str) -> i64 { 0 }
}

// ─── OrderRouter ──────────────────────────────────────────────────────────────

/// 订单路由器：将订单分发给对应市场类型的经纪商
pub struct OrderRouter {
    brokers: Vec<Box<dyn BrokerAdapter>>,
}

impl Default for OrderRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderRouter {
    pub fn new() -> Self {
        Self { brokers: Vec::new() }
    }

    /// 注册一个经纪商
    pub fn register(&mut self, broker: impl BrokerAdapter + 'static) {
        self.brokers.push(Box::new(broker));
    }

    /// 按市场类型查找第一个支持的经纪商
    fn find_broker(&self, market: MarketType) -> Option<&dyn BrokerAdapter> {
        self.brokers.iter().find(|b| b.supports(market)).map(|b| b.as_ref())
    }

    /// 路由并提交订单
    pub fn route(&self, order: &OrderSnapshot) -> OrderAck {
        match self.find_broker(order.market_type) {
            Some(broker) => broker.submit_order(order),
            None => OrderAck::rejected(
                &order.order_id,
                &format!("没有支持 {:?} 市场的经纪商", order.market_type),
            ),
        }
    }

    /// 路由撤单
    pub fn cancel(&self, order_id: &str, market: MarketType) -> Result<(), String> {
        match self.find_broker(market) {
            Some(broker) => broker.cancel_order(order_id),
            None => Err(format!("没有支持 {:?} 市场的经纪商", market)),
        }
    }

    pub fn broker_count(&self) -> usize {
        self.brokers.len()
    }
}

// ─── 委托记录（简单日志） ─────────────────────────────────────────────────────

/// 订单记录（用于审计追踪）
#[derive(Debug, Clone)]
pub struct OrderRecord {
    pub order: OrderSnapshot,
    pub ack: OrderAck,
    pub risk_approved: bool,
    pub block_reasons: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qarisk::market::MarketType;

    fn make_order(market: MarketType) -> OrderSnapshot {
        OrderSnapshot {
            order_id: "test_order".into(),
            instrument_id: "600000.XSHG".into(),
            direction: Direction::Buy,
            offset: Offset::Open,
            price: 10.0,
            volume: 100,
            market_type: market,
            account_id: "acc1".into(),
        }
    }

    #[test]
    fn test_mock_broker_submit() {
        let broker = MockBroker::universal();
        let order = make_order(MarketType::CN);
        let ack = broker.submit_order(&order);
        assert_eq!(ack.status, OrderStatus::Pending);
        assert_eq!(ack.order_id, "test_order");
    }

    #[test]
    fn test_router_found() {
        let mut router = OrderRouter::new();
        router.register(MockBroker::universal());
        let order = make_order(MarketType::CN);
        let ack = router.route(&order);
        assert_ne!(ack.status, OrderStatus::Rejected);
    }

    #[test]
    fn test_router_not_found() {
        let mut router = OrderRouter::new();
        router.register(MockBroker::new("cn_only", vec![MarketType::CN]));
        let order = make_order(MarketType::US); // 没有 US 经纪商
        let ack = router.route(&order);
        assert_eq!(ack.status, OrderStatus::Rejected);
    }

    #[test]
    fn test_router_cancel_no_broker() {
        let router = OrderRouter::new();
        let result = router.cancel("o1", MarketType::US);
        assert!(result.is_err());
    }
}
