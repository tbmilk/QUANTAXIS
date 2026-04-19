use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::qarisk::context::OrderSnapshot;
use crate::qarisk::execution::{BrokerAdapter, OrderAck, OrderStatus};
use crate::qarisk::market::MarketType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QmtBridgeMode {
    PythonBridge,
    NativeFfi,
}

#[derive(Debug, Clone)]
pub struct QmtBrokerConfig {
    pub account_id: String,
    pub endpoint: String,
    pub client_id: Option<String>,
    pub bridge_mode: QmtBridgeMode,
}

impl QmtBrokerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.account_id.trim().is_empty() {
            return Err("QMT account_id 不能为空".to_string());
        }
        if self.endpoint.trim().is_empty() {
            return Err("QMT endpoint 不能为空".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct QmtOrderRequest {
    pub account_id: String,
    pub order_id: String,
    pub instrument_id: String,
    pub price: f64,
    pub volume: i64,
    pub market_type: MarketType,
}

impl QmtOrderRequest {
    pub fn from_snapshot(config: &QmtBrokerConfig, order: &OrderSnapshot) -> Self {
        Self {
            account_id: config.account_id.clone(),
            order_id: order.order_id.clone(),
            instrument_id: order.instrument_id.clone(),
            price: order.price,
            volume: order.volume,
            market_type: order.market_type,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct QmtAdapterState {
    pub positions: HashMap<String, i64>,
    pub queued_orders: HashMap<String, QmtOrderRequest>,
    pub bridge_ready: bool,
}

pub struct QmtBrokerAdapter {
    config: QmtBrokerConfig,
    state: Arc<Mutex<QmtAdapterState>>,
    supported_markets: Vec<MarketType>,
}

impl QmtBrokerAdapter {
    pub fn new(config: QmtBrokerConfig) -> Result<Self, String> {
        config.validate()?;
        Ok(Self {
            config,
            state: Arc::new(Mutex::new(QmtAdapterState::default())),
            supported_markets: vec![MarketType::CN],
        })
    }

    pub fn mark_bridge_ready(&self, ready: bool) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "QMT 状态锁失败".to_string())?;
        state.bridge_ready = ready;
        Ok(())
    }

    pub fn snapshot_state(&self) -> QmtAdapterState {
        self.state.lock().map(|state| state.clone()).unwrap_or_default()
    }
}

impl BrokerAdapter for QmtBrokerAdapter {
    fn name(&self) -> &str {
        "QMT"
    }

    fn supported_markets(&self) -> &[MarketType] {
        &self.supported_markets
    }

    fn submit_order(&self, order: &OrderSnapshot) -> OrderAck {
        if order.market_type != MarketType::CN {
            return OrderAck::rejected(&order.order_id, "QMT 仅支持 A 股市场");
        }
        let request = QmtOrderRequest::from_snapshot(&self.config, order);
        match self.state.lock() {
            Ok(mut state) => {
                state
                    .queued_orders
                    .insert(request.order_id.clone(), request);
                let (status, message) = if state.bridge_ready {
                    (
                        OrderStatus::Pending,
                        format!("QMT {:?} bridge 已接受订单 {}", self.config.bridge_mode, order.order_id),
                    )
                } else {
                    (
                        OrderStatus::Pending,
                        format!("QMT bridge 尚未联机，订单 {} 已进入本地队列", order.order_id),
                    )
                };
                OrderAck {
                    order_id: order.order_id.clone(),
                    status,
                    message,
                    timestamp_ms: now_ms(),
                }
            }
            Err(_) => OrderAck::rejected(&order.order_id, "QMT 状态锁失败"),
        }
    }

    fn cancel_order(&self, order_id: &str) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "QMT 状态锁失败".to_string())?;
        if state.queued_orders.remove(order_id).is_some() {
            Ok(())
        } else {
            Err(format!("QMT 本地队列未找到订单 {}", order_id))
        }
    }

    fn query_position(&self, instrument_id: &str) -> i64 {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.positions.get(instrument_id).copied())
            .unwrap_or(0)
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qarisk::context::{Direction, Offset};

    #[test]
    fn test_qmt_submit_to_local_queue() {
        let broker = QmtBrokerAdapter::new(QmtBrokerConfig {
            account_id: "acc".to_string(),
            endpoint: "ipc:///tmp/qmt.sock".to_string(),
            client_id: None,
            bridge_mode: QmtBridgeMode::PythonBridge,
        })
        .unwrap();
        let ack = broker.submit_order(&OrderSnapshot {
            order_id: "qmt-1".to_string(),
            instrument_id: "SSE.600000".to_string(),
            direction: Direction::Buy,
            offset: Offset::Open,
            price: 12.3,
            volume: 100,
            market_type: MarketType::CN,
            account_id: "acc".to_string(),
        });
        assert_eq!(ack.status, OrderStatus::Pending);
        assert_eq!(broker.snapshot_state().queued_orders.len(), 1);
    }
}
