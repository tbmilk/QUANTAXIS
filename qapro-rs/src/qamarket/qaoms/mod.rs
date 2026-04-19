#![allow(dead_code)]
use crate::qaaccount::account::QA_Account;
use crate::qaaccount::order::QAOrder;
use crate::qaconnector::mongo::mongoclient::QAMongoClient;
use crate::qaenv::localenv::CONFIG;
use crate::qamarket::live_types::{OmsService, OmsSnapshot};
use crate::qarisk::context::{Direction, Offset, OrderSnapshot, PortfolioSnapshot, PositionSnapshot};
use crate::qarisk::execution::{OrderAck, OrderStatus, TradeReport};
use async_trait::async_trait;
use std::collections::HashMap;
pub struct QAOMS {
    pub accountmap: HashMap<String, QA_Account>,
    pub account_db: QAMongoClient,
    pub ordermap: HashMap<String, QAOrder>,
}

trait OrderCheck {
    fn add_main_account(&mut self, account_cookie: &str);
    fn add_sub_account(&mut self, sub_account_cookie: &str);
}
#[async_trait]
trait Reload {
    async fn init() -> Self;
    async fn reload_account(&mut self, _account: &str) {}
}

impl OrderCheck for QAOMS {
    fn add_main_account(&mut self, account_cookie: &str) {
        futures::executor::block_on(self.reload_account(account_cookie));
    }

    fn add_sub_account(&mut self, sub_account_cookie: &str) {
        futures::executor::block_on(self.reload_account(sub_account_cookie));
    }
}
#[async_trait]
impl Reload for QAOMS {
    async fn init() -> Self {
        Self {
            accountmap: HashMap::new(),
            account_db: QAMongoClient::new(&*CONFIG.account.uri).await,
            ordermap: Default::default(),
        }
    }
    async fn reload_account(&mut self, account_cookie: &str) {
        let account = self
            .account_db
            .get_account(account_cookie.parse().unwrap())
            .await;
        self.accountmap.insert(account_cookie.to_string(), account);
    }
}

/// 面向 `LiveContext` 的最小 OMS 实现。
///
/// 目标：
/// - 承接下单确认和成交回报
/// - 维护统一的订单/成交/持仓快照
/// - 先提供稳定的内存态语义，后续再补 Mongo/Redis 持久化
#[derive(Debug, Clone)]
pub struct MemoryOmsService {
    account_id: String,
    orders: HashMap<String, OrderAck>,
    order_snapshots: HashMap<String, OrderSnapshot>,
    trades: Vec<TradeReport>,
    portfolio: PortfolioSnapshot,
}

impl MemoryOmsService {
    pub fn new(account_id: &str) -> Self {
        Self {
            account_id: account_id.to_string(),
            orders: HashMap::new(),
            order_snapshots: HashMap::new(),
            trades: Vec::new(),
            portfolio: PortfolioSnapshot {
                account_id: account_id.to_string(),
                ..PortfolioSnapshot::default()
            },
        }
    }

    pub fn with_portfolio(mut self, portfolio: PortfolioSnapshot) -> Self {
        self.portfolio = portfolio;
        self
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    fn apply_trade_to_portfolio(&mut self, trade: &TradeReport) {
        let position = self
            .portfolio
            .positions
            .entry(trade.instrument_id.clone())
            .or_insert_with(|| PositionSnapshot {
                instrument_id: trade.instrument_id.clone(),
                ..PositionSnapshot::default()
            });

        match (trade.direction, trade.offset) {
            (Direction::Buy, Offset::Open) => {
                position.long_volume += trade.volume;
                position.long_avg_price =
                    weighted_average(position.long_avg_price, position.long_volume - trade.volume, trade.price, trade.volume);
            }
            (Direction::Sell, Offset::Open) => {
                position.short_volume += trade.volume;
                position.short_avg_price =
                    weighted_average(position.short_avg_price, position.short_volume - trade.volume, trade.price, trade.volume);
            }
            (Direction::Sell, _) => {
                position.long_volume = (position.long_volume - trade.volume).max(0);
            }
            (Direction::Buy, _) => {
                position.short_volume = (position.short_volume - trade.volume).max(0);
            }
        }

        position.market_value =
            (position.long_volume as f64 * trade.price) - (position.short_volume as f64 * trade.price);
        self.portfolio.unrealized_pnl = 0.0;
        self.portfolio.trade_count += 1;
        self.portfolio.trade_amount += trade.price * trade.volume as f64;
        self.portfolio.total_value = self.portfolio.cash
            + self
                .portfolio
                .positions
                .values()
                .map(|item| item.market_value)
                .sum::<f64>();
    }
}

impl Default for MemoryOmsService {
    fn default() -> Self {
        Self::new("")
    }
}

impl OmsService for MemoryOmsService {
    fn record_submit(&mut self, order: &OrderSnapshot, ack: &OrderAck) -> Result<(), String> {
        self.order_snapshots
            .insert(order.order_id.clone(), order.clone());
        self.orders.insert(order.order_id.clone(), ack.clone());
        Ok(())
    }

    fn apply_order_ack(&mut self, ack: &OrderAck) -> Result<(), String> {
        self.orders.insert(ack.order_id.clone(), ack.clone());
        Ok(())
    }

    fn apply_trade_report(&mut self, trade: &TradeReport) -> Result<(), String> {
        self.trades.push(trade.clone());
        self.apply_trade_to_portfolio(trade);

        if let Some(order_ack) = self.orders.get_mut(&trade.order_id) {
            order_ack.status = OrderStatus::Filled;
            order_ack.message = "成交完成".to_string();
            order_ack.timestamp_ms = trade.timestamp_ms;
        }
        Ok(())
    }

    fn reload_state(&mut self, account_id: &str) -> Result<(), String> {
        self.account_id = account_id.to_string();
        self.portfolio.account_id = account_id.to_string();
        Ok(())
    }

    fn snapshot(&self) -> OmsSnapshot {
        OmsSnapshot {
            orders: self.orders.clone(),
            trades: self.trades.clone(),
            portfolio: Some(self.portfolio.clone()),
        }
    }
}

fn weighted_average(old_price: f64, old_volume: i64, new_price: f64, new_volume: i64) -> f64 {
    let total_volume = old_volume + new_volume;
    if total_volume <= 0 {
        0.0
    } else {
        ((old_price * old_volume as f64) + (new_price * new_volume as f64)) / total_volume as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_oms_submit_and_trade() {
        let mut oms = MemoryOmsService::new("acc-1");
        let order = OrderSnapshot {
            order_id: "order-1".to_string(),
            instrument_id: "SHFE.ag2604".to_string(),
            direction: Direction::Buy,
            offset: Offset::Open,
            price: 100.0,
            volume: 2,
            market_type: crate::qarisk::market::MarketType::CNFutures,
            account_id: "acc-1".to_string(),
        };
        let ack = OrderAck {
            order_id: "order-1".to_string(),
            status: OrderStatus::Pending,
            message: "submitted".to_string(),
            timestamp_ms: 1,
        };
        oms.record_submit(&order, &ack).unwrap();
        oms.apply_trade_report(&TradeReport {
            order_id: "order-1".to_string(),
            instrument_id: "SHFE.ag2604".to_string(),
            direction: Direction::Buy,
            offset: Offset::Open,
            price: 101.0,
            volume: 2,
            commission: 1.0,
            timestamp_ms: 2,
        })
        .unwrap();

        let snapshot = oms.snapshot();
        assert_eq!(snapshot.orders["order-1"].status, OrderStatus::Filled);
        assert_eq!(
            snapshot
                .portfolio
                .as_ref()
                .unwrap()
                .positions["SHFE.ag2604"]
                .long_volume,
            2
        );
    }
}
