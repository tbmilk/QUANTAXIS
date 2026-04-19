use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::qarisk::context::OrderSnapshot;
use crate::qarisk::execution::{BrokerAdapter, OrderAck, OrderStatus};
use crate::qarisk::market::MarketType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XtpBridgeMode {
    NativeFfi,
    GatewayProcess,
}

#[derive(Debug, Clone)]
pub struct XtpBrokerConfig {
    pub account_id: String,
    pub endpoint: String,
    pub session_id: Option<String>,
    pub bridge_mode: XtpBridgeMode,
}

impl XtpBrokerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.account_id.trim().is_empty() {
            return Err("XTP account_id 不能为空".to_string());
        }
        if self.endpoint.trim().is_empty() {
            return Err("XTP endpoint 不能为空".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct XtpOrderRequest {
    pub account_id: String,
    pub order_id: String,
    pub instrument_id: String,
    pub price: f64,
    pub volume: i64,
    pub market_type: MarketType,
}

impl XtpOrderRequest {
    pub fn from_snapshot(config: &XtpBrokerConfig, order: &OrderSnapshot) -> Self {
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
pub struct XtpAdapterState {
    pub positions: HashMap<String, i64>,
    pub queued_orders: HashMap<String, XtpOrderRequest>,
    pub session_ready: bool,
}

pub struct XtpBrokerAdapter {
    config: XtpBrokerConfig,
    state: Arc<Mutex<XtpAdapterState>>,
    supported_markets: Vec<MarketType>,
}

impl XtpBrokerAdapter {
    pub fn new(config: XtpBrokerConfig) -> Result<Self, String> {
        config.validate()?;
        Ok(Self {
            config,
            state: Arc::new(Mutex::new(XtpAdapterState::default())),
            supported_markets: vec![MarketType::CN, MarketType::HK],
        })
    }

    pub fn mark_session_ready(&self, ready: bool) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "XTP 状态锁失败".to_string())?;
        state.session_ready = ready;
        Ok(())
    }

    pub fn snapshot_state(&self) -> XtpAdapterState {
        self.state.lock().map(|state| state.clone()).unwrap_or_default()
    }
}

impl BrokerAdapter for XtpBrokerAdapter {
    fn name(&self) -> &str {
        "XTP"
    }

    fn supported_markets(&self) -> &[MarketType] {
        &self.supported_markets
    }

    fn submit_order(&self, order: &OrderSnapshot) -> OrderAck {
        if !self.supports(order.market_type) {
            return OrderAck::rejected(
                &order.order_id,
                &format!("XTP 不支持 {:?} 市场", order.market_type),
            );
        }
        let request = XtpOrderRequest::from_snapshot(&self.config, order);
        match self.state.lock() {
            Ok(mut state) => {
                state
                    .queued_orders
                    .insert(request.order_id.clone(), request);
                let message = if state.session_ready {
                    format!("XTP {:?} 会话已接收订单 {}", self.config.bridge_mode, order.order_id)
                } else {
                    format!("XTP 会话未就绪，订单 {} 已缓存", order.order_id)
                };
                OrderAck {
                    order_id: order.order_id.clone(),
                    status: OrderStatus::Pending,
                    message,
                    timestamp_ms: now_ms(),
                }
            }
            Err(_) => OrderAck::rejected(&order.order_id, "XTP 状态锁失败"),
        }
    }

    fn cancel_order(&self, order_id: &str) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "XTP 状态锁失败".to_string())?;
        if state.queued_orders.remove(order_id).is_some() {
            Ok(())
        } else {
            Err(format!("XTP 本地队列未找到订单 {}", order_id))
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
    fn test_xtp_submit_hk_order() {
        let broker = XtpBrokerAdapter::new(XtpBrokerConfig {
            account_id: "acc".to_string(),
            endpoint: "tcp://127.0.0.1:6001".to_string(),
            session_id: None,
            bridge_mode: XtpBridgeMode::GatewayProcess,
        })
        .unwrap();
        let ack = broker.submit_order(&OrderSnapshot {
            order_id: "xtp-1".to_string(),
            instrument_id: "HK.00700".to_string(),
            direction: Direction::Buy,
            offset: Offset::Open,
            price: 320.0,
            volume: 100,
            market_type: MarketType::HK,
            account_id: "acc".to_string(),
        });
        assert_eq!(ack.status, OrderStatus::Pending);
        assert_eq!(broker.snapshot_state().queued_orders.len(), 1);
    }
}
