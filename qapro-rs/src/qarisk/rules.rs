//! 规则引擎（Phase 1）
//!
//! 插件式、优先级驱动、市场感知的风控规则框架。
//! 内置常用规则：持仓上限、集中度、涨跌停、杠杆、订单金额上限等。

use crate::qarisk::context::RiskContext;
use crate::qarisk::market::MarketType;

// ─── 规则结果 ─────────────────────────────────────────────────────────────────

/// 单条规则的评估结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleVerdict {
    /// 通过
    Pass,
    /// 警告（仍然放行，但需记录）
    Warn { reason: String },
    /// 拒绝（阻止订单）
    Block { reason: String },
}

impl RuleVerdict {
    pub fn is_blocked(&self) -> bool {
        matches!(self, RuleVerdict::Block { .. })
    }

    pub fn is_warning(&self) -> bool {
        matches!(self, RuleVerdict::Warn { .. })
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            RuleVerdict::Warn { reason } | RuleVerdict::Block { reason } => Some(reason),
            RuleVerdict::Pass => None,
        }
    }
}

// ─── 规则 Trait ───────────────────────────────────────────────────────────────

/// 单条风控规则（支持插件式扩展）
pub trait RiskRule: Send + Sync {
    /// 规则名称（全局唯一）
    fn name(&self) -> &str;

    /// 优先级：数字越小越先执行
    fn priority(&self) -> i32;

    /// 此规则适用于哪些市场类型（空 Vec 表示适用所有）
    fn applicable_markets(&self) -> &[MarketType] {
        &[]
    }

    /// 执行规则校验
    fn check(&self, ctx: &RiskContext) -> RuleVerdict;

    /// 是否对此上下文生效（默认：检查市场类型）
    fn is_applicable(&self, ctx: &RiskContext) -> bool {
        let markets = self.applicable_markets();
        markets.is_empty() || markets.contains(&ctx.order.market_type)
    }
}

// ─── 规则引擎 ─────────────────────────────────────────────────────────────────

/// 规则校验汇总结果
#[derive(Debug, Clone)]
pub struct EngineResult {
    /// 每条规则的（名称, 结果）
    pub verdicts: Vec<(String, RuleVerdict)>,
}

impl EngineResult {
    /// 整体是否通过（没有任何 Block）
    pub fn is_passed(&self) -> bool {
        !self.verdicts.iter().any(|(_, v)| v.is_blocked())
    }

    /// 所有拒绝原因
    pub fn block_reasons(&self) -> Vec<&str> {
        self.verdicts
            .iter()
            .filter_map(|(_, v)| if v.is_blocked() { v.reason() } else { None })
            .collect()
    }

    /// 所有警告原因
    pub fn warn_reasons(&self) -> Vec<&str> {
        self.verdicts
            .iter()
            .filter_map(|(_, v)| if v.is_warning() { v.reason() } else { None })
            .collect()
    }
}

/// 规则引擎：管理并按优先级执行所有注册规则
pub struct RuleEngine {
    rules: Vec<Box<dyn RiskRule>>,
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleEngine {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 注册一条规则
    pub fn register(&mut self, rule: impl RiskRule + 'static) {
        self.rules.push(Box::new(rule));
        // 保持优先级排序
        self.rules.sort_by_key(|r| r.priority());
    }

    /// 批量注册
    pub fn register_all(&mut self, rules: Vec<Box<dyn RiskRule>>) {
        for r in rules {
            self.rules.push(r);
        }
        self.rules.sort_by_key(|r| r.priority());
    }

    /// 评估所有适用规则，遇到第一个 Block 时提前退出（fast-fail）
    pub fn evaluate_fast_fail(&self, ctx: &RiskContext) -> EngineResult {
        let mut verdicts = Vec::new();
        for rule in &self.rules {
            if !rule.is_applicable(ctx) {
                continue;
            }
            let v = rule.check(ctx);
            let blocked = v.is_blocked();
            verdicts.push((rule.name().to_string(), v));
            if blocked {
                break;
            }
        }
        EngineResult { verdicts }
    }

    /// 评估所有适用规则（不提前退出，收集所有问题）
    pub fn evaluate_all(&self, ctx: &RiskContext) -> EngineResult {
        let verdicts = self.rules
            .iter()
            .filter(|r| r.is_applicable(ctx))
            .map(|r| (r.name().to_string(), r.check(ctx)))
            .collect();
        EngineResult { verdicts }
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

// ─── 内置规则 ─────────────────────────────────────────────────────────────────

/// 【内置】订单金额上限
pub struct MaxOrderValueRule {
    pub max_value: f64,
}

impl RiskRule for MaxOrderValueRule {
    fn name(&self) -> &str { "max_order_value" }
    fn priority(&self) -> i32 { 10 }

    fn check(&self, ctx: &RiskContext) -> RuleVerdict {
        if self.max_value <= 0.0 {
            return RuleVerdict::Pass;
        }
        let notional = ctx.order_notional();
        if notional > self.max_value {
            RuleVerdict::Block {
                reason: format!(
                    "订单金额 {:.2} 超过上限 {:.2}",
                    notional, self.max_value
                ),
            }
        } else {
            RuleVerdict::Pass
        }
    }
}

/// 【内置】单一合约持仓集中度上限
pub struct ConcentrationRule {
    pub max_concentration: f64,
}

impl RiskRule for ConcentrationRule {
    fn name(&self) -> &str { "concentration" }
    fn priority(&self) -> i32 { 20 }

    fn check(&self, ctx: &RiskContext) -> RuleVerdict {
        if ctx.would_exceed_concentration(self.max_concentration) {
            RuleVerdict::Block {
                reason: format!(
                    "持仓集中度将超过 {:.1}%",
                    self.max_concentration * 100.0
                ),
            }
        } else if ctx.portfolio.concentration(&ctx.order.instrument_id) > self.max_concentration * 0.8 {
            RuleVerdict::Warn {
                reason: format!(
                    "持仓集中度接近上限 {:.1}%",
                    self.max_concentration * 100.0
                ),
            }
        } else {
            RuleVerdict::Pass
        }
    }
}

/// 【内置】最大杠杆率限制
pub struct LeverageRule {
    pub max_leverage: f64,
}

impl RiskRule for LeverageRule {
    fn name(&self) -> &str { "leverage" }
    fn priority(&self) -> i32 { 30 }

    fn check(&self, ctx: &RiskContext) -> RuleVerdict {
        if ctx.portfolio.leverage > self.max_leverage {
            RuleVerdict::Block {
                reason: format!(
                    "当前杠杆率 {:.2} 超过上限 {:.2}",
                    ctx.portfolio.leverage, self.max_leverage
                ),
            }
        } else if ctx.portfolio.leverage > self.max_leverage * 0.9 {
            RuleVerdict::Warn {
                reason: format!(
                    "杠杆率 {:.2} 接近上限 {:.2}",
                    ctx.portfolio.leverage, self.max_leverage
                ),
            }
        } else {
            RuleVerdict::Pass
        }
    }
}

/// 【内置】涨跌停状态下的订单限制
pub struct LimitStatusRule;

impl RiskRule for LimitStatusRule {
    fn name(&self) -> &str { "limit_status" }
    fn priority(&self) -> i32 { 5 }

    fn check(&self, ctx: &RiskContext) -> RuleVerdict {
        use crate::qarisk::context::Direction;
        let inst = &ctx.order.instrument_id;
        // 涨停：禁止买入
        if ctx.market.is_upper_limit(inst) && ctx.order.direction == Direction::Buy {
            return RuleVerdict::Block {
                reason: format!("{} 已涨停，禁止买入", inst),
            };
        }
        // 跌停：禁止卖出
        if ctx.market.is_lower_limit(inst) && ctx.order.direction == Direction::Sell {
            return RuleVerdict::Block {
                reason: format!("{} 已跌停，禁止卖出", inst),
            };
        }
        RuleVerdict::Pass
    }
}

/// 【内置】非交易时段拦截
pub struct TradingHoursRule;

impl RiskRule for TradingHoursRule {
    fn name(&self) -> &str { "trading_hours" }
    fn priority(&self) -> i32 { 1 }

    fn check(&self, ctx: &RiskContext) -> RuleVerdict {
        if !ctx.market.is_trading {
            RuleVerdict::Block {
                reason: "当前非交易时段".to_string(),
            }
        } else {
            RuleVerdict::Pass
        }
    }
}

/// 【内置】资金充足性校验
pub struct CashAdequacyRule;

impl RiskRule for CashAdequacyRule {
    fn name(&self) -> &str { "cash_adequacy" }
    fn priority(&self) -> i32 { 15 }

    fn check(&self, ctx: &RiskContext) -> RuleVerdict {
        use crate::qarisk::context::{Direction, Offset};
        if ctx.order.direction == Direction::Buy && ctx.order.offset == Offset::Open {
            let required = ctx.order_notional();
            if ctx.portfolio.cash < required {
                return RuleVerdict::Block {
                    reason: format!(
                        "可用资金 {:.2} 不足，需要 {:.2}",
                        ctx.portfolio.cash, required
                    ),
                };
            }
            if ctx.portfolio.cash < required * 1.1 {
                return RuleVerdict::Warn {
                    reason: "资金余量不足 10%，请注意".to_string(),
                };
            }
        }
        RuleVerdict::Pass
    }
}

/// 【内置】单日成交次数上限
pub struct DailyTradeCountRule {
    pub max_count: u32,
}

impl RiskRule for DailyTradeCountRule {
    fn name(&self) -> &str { "daily_trade_count" }
    fn priority(&self) -> i32 { 40 }

    fn check(&self, ctx: &RiskContext) -> RuleVerdict {
        if ctx.portfolio.trade_count >= self.max_count {
            RuleVerdict::Block {
                reason: format!(
                    "当日成交次数 {} 已达上限 {}",
                    ctx.portfolio.trade_count, self.max_count
                ),
            }
        } else if ctx.portfolio.trade_count >= self.max_count.saturating_sub(5) {
            RuleVerdict::Warn {
                reason: format!(
                    "当日成交次数 {} 接近上限 {}",
                    ctx.portfolio.trade_count, self.max_count
                ),
            }
        } else {
            RuleVerdict::Pass
        }
    }
}

/// 【内置】单日最大亏损限制（净亏损触发限制）
pub struct DailyLossLimitRule {
    pub max_loss: f64,
}

impl RiskRule for DailyLossLimitRule {
    fn name(&self) -> &str { "daily_loss_limit" }
    fn priority(&self) -> i32 { 50 }

    fn check(&self, ctx: &RiskContext) -> RuleVerdict {
        let pnl = ctx.portfolio.realized_pnl + ctx.portfolio.unrealized_pnl;
        if pnl < -self.max_loss {
            RuleVerdict::Block {
                reason: format!(
                    "当日亏损 {:.2} 超过限额 {:.2}，停止交易",
                    -pnl, self.max_loss
                ),
            }
        } else if pnl < -self.max_loss * 0.8 {
            RuleVerdict::Warn {
                reason: format!(
                    "当日亏损 {:.2} 接近限额 {:.2}",
                    -pnl, self.max_loss
                ),
            }
        } else {
            RuleVerdict::Pass
        }
    }
}

/// 构建内置规则的默认规则引擎
pub fn default_rule_engine(market_type: MarketType) -> RuleEngine {
    let profile = crate::qarisk::market::MarketProfile::for_market(market_type);
    let mut engine = RuleEngine::new();
    engine.register(TradingHoursRule);
    engine.register(LimitStatusRule);
    engine.register(MaxOrderValueRule { max_value: profile.max_order_value });
    engine.register(CashAdequacyRule);
    engine.register(ConcentrationRule { max_concentration: profile.max_concentration });
    engine.register(LeverageRule { max_leverage: profile.max_leverage });
    engine.register(DailyTradeCountRule { max_count: 500 });
    engine.register(DailyLossLimitRule { max_loss: 50_000.0 });
    engine
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::qarisk::context::*;
    use crate::qarisk::market::MarketType;

    fn make_ctx(price: f64, cash: f64, is_trading: bool) -> RiskContext {
        let order = OrderSnapshot {
            order_id: "o1".into(),
            instrument_id: "600000.XSHG".into(),
            direction: Direction::Buy,
            offset: Offset::Open,
            price,
            volume: 100,
            market_type: MarketType::CN,
            account_id: "acc1".into(),
        };
        let mut portfolio = PortfolioSnapshot::default();
        portfolio.cash = cash;
        portfolio.total_value = 1_000_000.0;
        let market = MarketState {
            prices: {
                let mut m = HashMap::new();
                m.insert("600000.XSHG".into(), price);
                m
            },
            is_trading,
            ..Default::default()
        };
        RiskContext::new(order, portfolio, market)
    }

    #[test]
    fn test_trading_hours_block() {
        let ctx = make_ctx(10.0, 100_000.0, false);
        let rule = TradingHoursRule;
        assert!(rule.check(&ctx).is_blocked());
    }

    #[test]
    fn test_cash_adequacy_pass() {
        let ctx = make_ctx(10.0, 100_000.0, true);
        let rule = CashAdequacyRule;
        assert_eq!(rule.check(&ctx), RuleVerdict::Pass);
    }

    #[test]
    fn test_cash_adequacy_block() {
        let ctx = make_ctx(10.0, 500.0, true); // 需要 1000，只有 500
        let rule = CashAdequacyRule;
        assert!(rule.check(&ctx).is_blocked());
    }

    #[test]
    fn test_max_order_value_block() {
        let ctx = make_ctx(100.0, 1_000_000.0, true); // 100×100=10000
        let rule = MaxOrderValueRule { max_value: 5_000.0 };
        assert!(rule.check(&ctx).is_blocked());
    }

    #[test]
    fn test_engine_fast_fail() {
        let ctx = make_ctx(10.0, 500.0, true); // 资金不足
        let mut engine = RuleEngine::new();
        engine.register(TradingHoursRule);
        engine.register(CashAdequacyRule);
        let res = engine.evaluate_fast_fail(&ctx);
        assert!(!res.is_passed());
        // fast-fail 模式下遇到 Block 后停止
        let blocks = res.block_reasons();
        assert!(!blocks.is_empty());
    }

    #[test]
    fn test_daily_loss_limit() {
        let mut ctx = make_ctx(10.0, 100_000.0, true);
        ctx.portfolio.realized_pnl = -60_000.0;
        let rule = DailyLossLimitRule { max_loss: 50_000.0 };
        assert!(rule.check(&ctx).is_blocked());
    }

    #[test]
    fn test_default_engine_cn() {
        let engine = default_rule_engine(MarketType::CN);
        assert!(engine.rule_count() >= 5);
    }
}
