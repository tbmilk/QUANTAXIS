//! 风控上下文（Phase 1）
//!
//! `RiskContext` 封装单次风控评估所需的所有状态快照：
//! 待校验订单、投资组合持仓、市场行情、历史数据、预测结果、订单簿。

use std::collections::HashMap;

use crate::qarisk::market::MarketType;

// ─── 订单相关类型 ─────────────────────────────────────────────────────────────

/// 买卖方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Buy,
    Sell,
}

/// 开平仓标志
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offset {
    Open,
    Close,
    CloseToday,
    CloseYesterday,
}

/// 待校验的订单快照
#[derive(Debug, Clone)]
pub struct OrderSnapshot {
    pub order_id: String,
    pub instrument_id: String,
    pub direction: Direction,
    pub offset: Offset,
    /// 报价
    pub price: f64,
    /// 报量（手）
    pub volume: i64,
    pub market_type: MarketType,
    pub account_id: String,
}

impl OrderSnapshot {
    /// 订单名义价值（price × volume）
    pub fn notional(&self) -> f64 {
        self.price * self.volume as f64
    }

    pub fn is_buy(&self) -> bool {
        self.direction == Direction::Buy
    }

    pub fn is_open(&self) -> bool {
        self.offset == Offset::Open
    }
}

// ─── 投资组合类型 ─────────────────────────────────────────────────────────────

/// 单一合约持仓快照
#[derive(Debug, Clone, Default)]
pub struct PositionSnapshot {
    pub instrument_id: String,
    pub long_volume: i64,
    pub short_volume: i64,
    pub long_avg_price: f64,
    pub short_avg_price: f64,
    /// 按最新价计算的市值
    pub market_value: f64,
}

impl PositionSnapshot {
    /// 净持仓（多 - 空）
    pub fn net_volume(&self) -> i64 {
        self.long_volume - self.short_volume
    }
}

/// 投资组合快照
#[derive(Debug, Clone)]
pub struct PortfolioSnapshot {
    pub account_id: String,
    pub positions: HashMap<String, PositionSnapshot>,
    /// 可用资金
    pub cash: f64,
    /// 总资产（cash + 持仓市值）
    pub total_value: f64,
    /// 当前杠杆率（持仓名义价值 / 净资产）
    pub leverage: f64,
    /// 当日已实现盈亏
    pub realized_pnl: f64,
    /// 浮动盈亏
    pub unrealized_pnl: f64,
    /// 当日成交笔数
    pub trade_count: u32,
    /// 当日累计成交金额
    pub trade_amount: f64,
}

impl Default for PortfolioSnapshot {
    fn default() -> Self {
        Self {
            account_id: String::new(),
            positions: HashMap::new(),
            cash: 0.0,
            total_value: 0.0,
            leverage: 0.0,
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            trade_count: 0,
            trade_amount: 0.0,
        }
    }
}

impl PortfolioSnapshot {
    /// 持仓集中度：某合约市值 / 总资产
    pub fn concentration(&self, instrument_id: &str) -> f64 {
        if self.total_value == 0.0 {
            return 0.0;
        }
        let mv = self.positions.get(instrument_id).map(|p| p.market_value.abs()).unwrap_or(0.0);
        mv / self.total_value
    }

    /// 现有持仓中是否持有该合约（多头）
    pub fn has_long(&self, instrument_id: &str) -> bool {
        self.positions.get(instrument_id).map(|p| p.long_volume > 0).unwrap_or(false)
    }

    /// 持仓数量（仅多头）
    pub fn long_volume(&self, instrument_id: &str) -> i64 {
        self.positions.get(instrument_id).map(|p| p.long_volume).unwrap_or(0)
    }

    /// 持仓数量（仅空头）
    pub fn short_volume(&self, instrument_id: &str) -> i64 {
        self.positions.get(instrument_id).map(|p| p.short_volume).unwrap_or(0)
    }
}

// ─── 市场状态 ─────────────────────────────────────────────────────────────────

/// 涨跌停状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitStatus {
    UpperLimit,
    LowerLimit,
    Normal,
}

/// 市场行情状态
#[derive(Debug, Clone, Default)]
pub struct MarketState {
    /// 最新成交价
    pub prices: HashMap<String, f64>,
    /// 当日成交量
    pub volumes: HashMap<String, i64>,
    /// 前收盘价（用于涨跌停计算）
    pub prev_close: HashMap<String, f64>,
    /// 是否在交易时段
    pub is_trading: bool,
    /// 涨跌停状态
    pub limit_status: HashMap<String, LimitStatus>,
    pub market_type: MarketType,
}

impl MarketState {
    pub fn last_price(&self, instrument_id: &str) -> Option<f64> {
        self.prices.get(instrument_id).copied()
    }

    pub fn is_limit(&self, instrument_id: &str) -> bool {
        matches!(
            self.limit_status.get(instrument_id),
            Some(LimitStatus::UpperLimit) | Some(LimitStatus::LowerLimit)
        )
    }

    pub fn is_upper_limit(&self, instrument_id: &str) -> bool {
        matches!(self.limit_status.get(instrument_id), Some(LimitStatus::UpperLimit))
    }

    pub fn is_lower_limit(&self, instrument_id: &str) -> bool {
        matches!(self.limit_status.get(instrument_id), Some(LimitStatus::LowerLimit))
    }
}

// ─── 数据上下文 ───────────────────────────────────────────────────────────────

/// 历史 / 参考数据上下文
#[derive(Debug, Clone, Default)]
pub struct DataContext {
    /// 各资产近 N 日日收益率（instrument_id -> [r1, r2, ...]，最新在末尾）
    pub historical_returns: HashMap<String, Vec<f64>>,
    /// 基准指数收益率
    pub benchmark_returns: Vec<f64>,
    /// 因子敞口（factor_name -> [exposure_per_asset]）
    pub factor_exposures: HashMap<String, Vec<f64>>,
    /// 资产 ID 列表（与 factor_exposures 向量对齐）
    pub asset_ids: Vec<String>,
}

// ─── 预测结果 ─────────────────────────────────────────────────────────────────

/// 市场状态（Regime）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketRegime {
    Bull,
    Bear,
    Sideways,
}

/// 风险预测结果
#[derive(Debug, Clone)]
pub struct ForecastResult {
    /// 各资产预测年化波动率
    pub predicted_vols: HashMap<String, f64>,
    /// 投资组合预测波动率（年化）
    pub portfolio_vol: f64,
    pub regime: MarketRegime,
    /// 95% VaR（负数，表示损失）
    pub var_95: f64,
    /// 95% Expected Shortfall（负数）
    pub es_95: f64,
}

// ─── 订单簿 ───────────────────────────────────────────────────────────────────

/// Level-2 订单簿快照
#[derive(Debug, Clone, Default)]
pub struct OrderBook {
    pub instrument_id: String,
    /// 买方报价：(价格, 量)，按价格降序
    pub bids: Vec<(f64, i64)>,
    /// 卖方报价：(价格, 量)，按价格升序
    pub asks: Vec<(f64, i64)>,
    pub timestamp_ms: i64,
}

impl OrderBook {
    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first().map(|(p, _)| *p)
    }

    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first().map(|(p, _)| *p)
    }

    pub fn mid_price(&self) -> Option<f64> {
        Some((self.best_bid()? + self.best_ask()?) / 2.0)
    }

    pub fn spread(&self) -> Option<f64> {
        Some(self.best_ask()? - self.best_bid()?)
    }

    /// 买方前 `levels` 档总量
    pub fn bid_depth(&self, levels: usize) -> i64 {
        self.bids.iter().take(levels).map(|(_, v)| *v).sum()
    }

    /// 卖方前 `levels` 档总量
    pub fn ask_depth(&self, levels: usize) -> i64 {
        self.asks.iter().take(levels).map(|(_, v)| *v).sum()
    }
}

// ─── 风控上下文（核心） ────────────────────────────────────────────────────────

/// 单次风控评估的完整上下文
#[derive(Debug, Clone)]
pub struct RiskContext {
    pub order: OrderSnapshot,
    pub portfolio: PortfolioSnapshot,
    pub market: MarketState,
    pub data: DataContext,
    pub forecast: Option<ForecastResult>,
    pub orderbook: Option<OrderBook>,
}

impl RiskContext {
    /// 便捷构造函数（最小字段）
    pub fn new(order: OrderSnapshot, portfolio: PortfolioSnapshot, market: MarketState) -> Self {
        Self {
            order,
            portfolio,
            market,
            data: DataContext::default(),
            forecast: None,
            orderbook: None,
        }
    }

    pub fn with_data(mut self, data: DataContext) -> Self {
        self.data = data;
        self
    }

    pub fn with_forecast(mut self, forecast: ForecastResult) -> Self {
        self.forecast = Some(forecast);
        self
    }

    pub fn with_orderbook(mut self, ob: OrderBook) -> Self {
        self.orderbook = Some(ob);
        self
    }

    /// 订单名义价值
    pub fn order_notional(&self) -> f64 {
        self.order.notional()
    }

    /// 本次下单后持仓会超过资产的 max_concentration 吗？
    pub fn would_exceed_concentration(&self, max_concentration: f64) -> bool {
        let price = self.market.last_price(&self.order.instrument_id)
            .unwrap_or(self.order.price);
        let add_volume = if self.order.is_open() { self.order.volume } else { 0 };
        let existing = self.portfolio.long_volume(&self.order.instrument_id);
        let new_mv = (existing + add_volume) as f64 * price;
        let total = self.portfolio.total_value.max(1.0);
        new_mv / total > max_concentration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context() -> RiskContext {
        let order = OrderSnapshot {
            order_id: "o1".into(),
            instrument_id: "600000.XSHG".into(),
            direction: Direction::Buy,
            offset: Offset::Open,
            price: 10.0,
            volume: 100,
            market_type: MarketType::CN,
            account_id: "acc1".into(),
        };
        let mut portfolio = PortfolioSnapshot::default();
        portfolio.total_value = 100_000.0;
        portfolio.cash = 100_000.0;
        let market = MarketState {
            prices: {
                let mut m = HashMap::new();
                m.insert("600000.XSHG".into(), 10.0);
                m
            },
            is_trading: true,
            ..Default::default()
        };
        RiskContext::new(order, portfolio, market)
    }

    #[test]
    fn test_order_notional() {
        let ctx = make_context();
        assert_eq!(ctx.order_notional(), 1000.0);
    }

    #[test]
    fn test_concentration_within_limit() {
        let ctx = make_context();
        // 100 手 × 10 元 = 1000 元，占 100000 = 1%，小于 30%
        assert!(!ctx.would_exceed_concentration(0.30));
    }

    #[test]
    fn test_orderbook_spread() {
        let ob = OrderBook {
            instrument_id: "rb2501".into(),
            bids: vec![(3498.0, 20), (3496.0, 50)],
            asks: vec![(3500.0, 10), (3502.0, 30)],
            timestamp_ms: 0,
        };
        assert_eq!(ob.spread(), Some(2.0));
        assert_eq!(ob.bid_depth(2), 70);
    }
}
