use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::qaprotocol::qifi::account::{Order as QifiOrder, QIFI};
use crate::qarisk::context::{Direction, Offset, OrderSnapshot};
use crate::qarisk::execution::{BrokerAdapter, OrderAck, OrderStatus, TradeReport};
use crate::qarisk::market::MarketType;
pub use crate::qatrader::qatrader::QATrader;

#[derive(Debug, Clone)]
pub struct QifiBrokerConfig {
    pub account_id: String,
    pub broker_name: String,
    pub ws_uri: Option<String>,
    pub eventmq_ip: Option<String>,
}

impl QifiBrokerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.account_id.trim().is_empty() {
            return Err("account_id 不能为空".to_string());
        }
        if self.broker_name.trim().is_empty() {
            return Err("broker_name 不能为空".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct QifiOrderRequest {
    pub account_id: String,
    pub order_id: String,
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub volume: i64,
    pub price: f64,
    pub market_type: MarketType,
}

impl QifiOrderRequest {
    pub fn from_snapshot(config: &QifiBrokerConfig, order: &OrderSnapshot) -> Self {
        Self {
            account_id: config.account_id.clone(),
            order_id: order.order_id.clone(),
            instrument_id: order.instrument_id.clone(),
            direction: direction_to_qifi(order.direction).to_string(),
            offset: offset_to_qifi(order.offset).to_string(),
            volume: order.volume,
            price: order.price,
            market_type: order.market_type,
        }
    }
}

#[derive(Debug, Clone)]
pub struct QifiAdapterState {
    pub positions: HashMap<String, i64>,
    pub order_requests: HashMap<String, QifiOrderRequest>,
    pub last_error: Option<String>,
}

impl Default for QifiAdapterState {
    fn default() -> Self {
        Self {
            positions: HashMap::new(),
            order_requests: HashMap::new(),
            last_error: None,
        }
    }
}

pub struct QifiBrokerAdapter {
    config: QifiBrokerConfig,
    state: Arc<Mutex<QifiAdapterState>>,
    supported_markets: Vec<MarketType>,
}

impl QifiBrokerAdapter {
    pub fn new(config: QifiBrokerConfig, supported_markets: Vec<MarketType>) -> Result<Self, String> {
        config.validate()?;
        Ok(Self {
            config,
            state: Arc::new(Mutex::new(QifiAdapterState::default())),
            supported_markets,
        })
    }

    pub fn universal(config: QifiBrokerConfig) -> Result<Self, String> {
        Self::new(
            config,
            vec![
                MarketType::CN,
                MarketType::CNFutures,
                MarketType::HK,
                MarketType::US,
                MarketType::Crypto,
            ],
        )
    }

    pub fn from_qifi(config: QifiBrokerConfig, qifi: &QIFI) -> Result<Self, String> {
        let adapter = Self::universal(config)?;
        let mut state = adapter.state.lock().map_err(|_| "QifiAdapterState 加锁失败".to_string())?;
        for (instrument_id, position) in &qifi.positions {
            let net = (position.volume_long - position.volume_short) as i64;
            state.positions.insert(instrument_id.clone(), net);
        }
        drop(state);
        Ok(adapter)
    }

    pub fn snapshot_state(&self) -> QifiAdapterState {
        self.state.lock().map(|state| state.clone()).unwrap_or_default()
    }

    pub fn apply_trade_report(&self, trade: &TradeReport) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "QifiAdapterState 加锁失败".to_string())?;
        let entry = state.positions.entry(trade.instrument_id.clone()).or_insert(0);
        let signed_volume = signed_volume_from_trade(trade.direction, trade.offset, trade.volume);
        *entry += signed_volume;
        Ok(())
    }

    pub fn ack_from_qifi_order(order: &QifiOrder) -> OrderAck {
        let status = qifi_status_to_order_status(order.status.as_str());
        OrderAck {
            order_id: order.order_id.clone(),
            status,
            message: order.last_msg.clone(),
            timestamp_ms: order.insert_date_time,
        }
    }
}

impl BrokerAdapter for QifiBrokerAdapter {
    fn name(&self) -> &str {
        &self.config.broker_name
    }

    fn supported_markets(&self) -> &[MarketType] {
        &self.supported_markets
    }

    fn submit_order(&self, order: &OrderSnapshot) -> OrderAck {
        if !self.supports(order.market_type) {
            return OrderAck::rejected(
                &order.order_id,
                &format!("QIFI adapter 不支持 {:?} 市场", order.market_type),
            );
        }

        let request = QifiOrderRequest::from_snapshot(&self.config, order);
        match self.state.lock() {
            Ok(mut state) => {
                state
                    .order_requests
                    .insert(request.order_id.clone(), request);
                OrderAck {
                    order_id: order.order_id.clone(),
                    status: OrderStatus::Pending,
                    message: format!("QIFI bridge 已接受订单 {}", order.order_id),
                    timestamp_ms: now_ms(),
                }
            }
            Err(_) => OrderAck::rejected(&order.order_id, "QIFI adapter 状态锁失败"),
        }
    }

    fn cancel_order(&self, order_id: &str) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "QifiAdapterState 加锁失败".to_string())?;
        if state.order_requests.remove(order_id).is_some() {
            Ok(())
        } else {
            Err(format!("QIFI adapter 中未找到订单 {}", order_id))
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

fn direction_to_qifi(direction: Direction) -> &'static str {
    match direction {
        Direction::Buy => "BUY",
        Direction::Sell => "SELL",
    }
}

fn offset_to_qifi(offset: Offset) -> &'static str {
    match offset {
        Offset::Open => "OPEN",
        Offset::Close => "CLOSE",
        Offset::CloseToday => "CLOSETODAY",
        Offset::CloseYesterday => "CLOSEYESTERDAY",
    }
}

fn qifi_status_to_order_status(status: &str) -> OrderStatus {
    match status {
        "ALIVE" | "PENDING" | "QUEUED" => OrderStatus::Pending,
        "PARTIAL" | "PARTIAL_FILLED" => OrderStatus::PartialFilled,
        "FINISHED" | "FILLED" => OrderStatus::Filled,
        "CANCELLED" | "CANCELED" => OrderStatus::Cancelled,
        _ => OrderStatus::Rejected,
    }
}

fn signed_volume_from_trade(direction: Direction, offset: Offset, volume: i64) -> i64 {
    match (direction, offset) {
        (Direction::Buy, Offset::Open) => volume,
        (Direction::Sell, Offset::Open) => -volume,
        (Direction::Sell, _) => -volume,
        (Direction::Buy, _) => volume,
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

    fn make_order(market_type: MarketType) -> OrderSnapshot {
        OrderSnapshot {
            order_id: "order-1".to_string(),
            instrument_id: "SHFE.ag2604".to_string(),
            direction: Direction::Buy,
            offset: Offset::Open,
            price: 100.0,
            volume: 2,
            market_type,
            account_id: "acc-1".to_string(),
        }
    }

    #[test]
    fn test_qifi_submit_records_request() {
        let adapter = QifiBrokerAdapter::universal(QifiBrokerConfig {
            account_id: "acc-1".to_string(),
            broker_name: "qifi".to_string(),
            ws_uri: None,
            eventmq_ip: None,
        })
        .unwrap();
        let ack = adapter.submit_order(&make_order(MarketType::CNFutures));
        assert_eq!(ack.status, OrderStatus::Pending);
        assert_eq!(adapter.snapshot_state().order_requests.len(), 1);
    }

    #[test]
    fn test_qifi_apply_trade_updates_position() {
        let adapter = QifiBrokerAdapter::universal(QifiBrokerConfig {
            account_id: "acc-1".to_string(),
            broker_name: "qifi".to_string(),
            ws_uri: None,
            eventmq_ip: None,
        })
        .unwrap();
        adapter
            .apply_trade_report(&TradeReport {
                order_id: "o1".to_string(),
                instrument_id: "SHFE.ag2604".to_string(),
                direction: Direction::Buy,
                offset: Offset::Open,
                price: 100.0,
                volume: 3,
                commission: 1.0,
                timestamp_ms: 1,
            })
            .unwrap();
        assert_eq!(adapter.query_position("SHFE.ag2604"), 3);
    }
}
