//! 实盘统一接口骨架。
//!
//! 目标：
//! - 在接入 openctp / QMT 之前先冻结主链路接口
//! - 让实时数据源、历史回放源、不同柜台共享同一套中间层

use std::collections::HashMap;

use crate::qadatastruct::mdsnapshot::MDSnapshot;
use crate::qamarket::qamdgateway::MarketDataSource as GatewayMarketDataSource;
use crate::qarisk::context::{OrderSnapshot, PortfolioSnapshot};
use crate::qarisk::execution::{BrokerAdapter, OrderAck, TradeReport};
use crate::qarisk::service::{RiskDecision, RiskService};

/// 行情源健康状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceHealth {
    Healthy,
    Degraded,
    Down,
}

/// 行情源统一事件
#[derive(Debug, Clone)]
pub struct MarketDataEnvelope {
    pub source: GatewayMarketDataSource,
    pub snapshot: MDSnapshot,
    pub replay: bool,
}

/// 统一行情源接口
pub trait MarketDataSource: Send {
    fn name(&self) -> &str;
    fn source_type(&self) -> GatewayMarketDataSource;
    fn health_check(&self) -> SourceHealth;
    fn subscribe(&mut self, instruments: &[String]) -> Result<(), String>;
    fn unsubscribe(&mut self, instruments: &[String]) -> Result<(), String>;
}

/// 统一回放/轮询接口
pub trait MarketDataPullSource: MarketDataSource {
    fn next_event(&mut self) -> Result<Option<MarketDataEnvelope>, String>;
}

/// OMS 侧统一快照
#[derive(Debug, Clone, Default)]
pub struct OmsSnapshot {
    pub orders: HashMap<String, OrderAck>,
    pub trades: Vec<TradeReport>,
    pub portfolio: Option<PortfolioSnapshot>,
}

/// OMS 统一服务接口
pub trait OmsService: Send {
    fn record_submit(&mut self, order: &OrderSnapshot, ack: &OrderAck) -> Result<(), String>;
    fn apply_order_ack(&mut self, ack: &OrderAck) -> Result<(), String>;
    fn apply_trade_report(&mut self, trade: &TradeReport) -> Result<(), String>;
    fn reload_state(&mut self, account_id: &str) -> Result<(), String>;
    fn snapshot(&self) -> OmsSnapshot;
}

/// 信号结构
#[derive(Debug, Clone)]
pub struct Signal {
    pub instrument_id: String,
    pub order: OrderSnapshot,
    pub source: String,
    pub strength: f64,
}

/// 策略统一事件接口
pub trait SignalGenerator: Send {
    fn name(&self) -> &str;
    fn on_snapshot(&mut self, _snapshot: &MDSnapshot) -> Result<Vec<Signal>, String> {
        Ok(Vec::new())
    }
    fn on_order_ack(&mut self, _ack: &OrderAck) -> Result<(), String> {
        Ok(())
    }
    fn on_trade_report(&mut self, _trade: &TradeReport) -> Result<(), String> {
        Ok(())
    }
}

/// 实盘运行上下文
pub struct LiveContext<'a> {
    pub risk_service: &'a RiskService,
    pub broker: &'a dyn BrokerAdapter,
    pub oms: &'a mut dyn OmsService,
}

impl<'a> LiveContext<'a> {
    pub fn evaluate_and_submit(
        &mut self,
        order: &OrderSnapshot,
        decision: &RiskDecision,
    ) -> Result<OrderAck, String> {
        if !decision.approved {
            return Err(format!("risk blocked: {}", decision.block_reasons.join(",")));
        }
        let ack = self.broker.submit_order(order);
        self.oms.record_submit(order, &ack)?;
        Ok(ack)
    }

    pub fn handle_order_ack(&mut self, ack: &OrderAck) -> Result<(), String> {
        self.oms.apply_order_ack(ack)
    }

    pub fn handle_trade_report(&mut self, trade: &TradeReport) -> Result<(), String> {
        self.oms.apply_trade_report(trade)
    }
}

