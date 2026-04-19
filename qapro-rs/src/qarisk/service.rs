//! 风险服务（Phase 1）
//!
//! `RiskService` 是风控系统的主入口，串联：
//! 状态机 → Kill Switch 检查 → 规则引擎评估 → 结果上报

use crate::qarisk::context::RiskContext;
use crate::qarisk::market::MarketType;
use crate::qarisk::rules::{default_rule_engine, EngineResult, RuleEngine};
use crate::qarisk::statemachine::{RiskLevel, RiskStateMachine};
use crate::qarisk::config::KillSwitch;

// ─── 服务评估结果 ─────────────────────────────────────────────────────────────

/// 风控服务对单笔订单的最终判断
#[derive(Debug, Clone)]
pub struct RiskDecision {
    /// 是否放行
    pub approved: bool,
    /// 当前风险等级
    pub risk_level: RiskLevel,
    /// 拒绝原因列表（`approved = false` 时不为空）
    pub block_reasons: Vec<String>,
    /// 警告信息列表
    pub warnings: Vec<String>,
}

impl RiskDecision {
    fn approved(level: RiskLevel, warnings: Vec<String>) -> Self {
        Self { approved: true, risk_level: level, block_reasons: vec![], warnings }
    }

    fn blocked(level: RiskLevel, reasons: Vec<String>, warnings: Vec<String>) -> Self {
        Self { approved: false, risk_level: level, block_reasons: reasons, warnings }
    }
}

// ─── 风险服务 ─────────────────────────────────────────────────────────────────

/// 风险服务：单线程版本（适合嵌入回测循环或单账户实盘）
pub struct RiskService {
    state_machine: RiskStateMachine,
    rule_engine: RuleEngine,
    kill_switch: KillSwitch,
    /// 持仓初始总资产（用于计算 PnL 比例）
    initial_capital: f64,
    market_type: MarketType,
}

impl RiskService {
    /// 使用默认内置规则构建服务
    pub fn new(market_type: MarketType, initial_capital: f64) -> Self {
        Self {
            state_machine: RiskStateMachine::new(),
            rule_engine: default_rule_engine(market_type),
            kill_switch: KillSwitch::new(),
            initial_capital,
            market_type,
        }
    }

    /// 使用自定义规则引擎构建服务
    pub fn with_rule_engine(
        market_type: MarketType,
        initial_capital: f64,
        rule_engine: RuleEngine,
    ) -> Self {
        Self {
            state_machine: RiskStateMachine::new(),
            rule_engine,
            kill_switch: KillSwitch::new(),
            initial_capital,
            market_type,
        }
    }

    /// 获取共享的 Kill Switch（供外部触发）
    pub fn kill_switch(&self) -> KillSwitch {
        self.kill_switch.share()
    }

    /// 当前风险等级
    pub fn risk_level(&self) -> RiskLevel {
        self.state_machine.current_level()
    }

    /// 日初重置（清除当日状态）
    pub fn daily_reset(&mut self, timestamp_ms: i64) {
        self.state_machine.reset(timestamp_ms);
    }

    /// 周期性风险更新（每个 tick/bar 调用一次）
    ///
    /// - `current_value`：当前账户总资产
    /// - `portfolio_vol`：最新预测年化波动率（可由 ForecastEngine 提供）
    pub fn update(&mut self, current_value: f64, portfolio_vol: f64, timestamp_ms: i64) {
        if self.initial_capital <= 0.0 {
            return;
        }
        let pnl_ratio = (current_value - self.initial_capital) / self.initial_capital;
        self.state_machine.auto_evaluate(pnl_ratio, portfolio_vol, timestamp_ms);
    }

    /// 评估一笔订单（核心方法）
    pub fn evaluate(&self, ctx: &RiskContext) -> RiskDecision {
        // 1. Kill Switch 检查（最高优先级）
        if self.kill_switch.is_triggered() {
            return RiskDecision::blocked(
                RiskLevel::Halt,
                vec!["Kill Switch 已触发，所有交易停止".to_string()],
                vec![],
            );
        }

        let level = self.state_machine.current_level();

        // 2. HALT 状态：拒绝一切
        if !self.state_machine.can_trade() {
            return RiskDecision::blocked(
                level,
                vec!["风险等级 HALT，所有交易已停止".to_string()],
                vec![],
            );
        }

        // 3. LIQUIDATE/RESTRICT 状态：只允许减仓
        if !self.state_machine.can_open() && ctx.order.is_open() {
            return RiskDecision::blocked(
                level,
                vec![format!("风险等级 {}，禁止开仓，仅允许平仓", level)],
                vec![],
            );
        }

        // 4. 规则引擎评估
        let engine_result: EngineResult = self.rule_engine.evaluate_all(ctx);

        let warnings: Vec<String> = engine_result
            .warn_reasons()
            .iter()
            .map(|s| s.to_string())
            .collect();

        if engine_result.is_passed() {
            RiskDecision::approved(level, warnings)
        } else {
            let block_reasons: Vec<String> = engine_result
                .block_reasons()
                .iter()
                .map(|s| s.to_string())
                .collect();
            RiskDecision::blocked(level, block_reasons, warnings)
        }
    }

    /// 手动触发 Kill Switch
    pub fn trigger_kill_switch(&self) {
        self.kill_switch.trigger();
    }

    /// 手动将状态提升到指定等级
    pub fn escalate(&mut self, level: RiskLevel, reason: &str, timestamp_ms: i64) {
        self.state_machine.escalate(level, reason, timestamp_ms);
    }

    pub fn market_type(&self) -> MarketType {
        self.market_type
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::qarisk::context::*;
    use crate::qarisk::market::MarketType;

    fn make_ctx(cash: f64, is_trading: bool) -> RiskContext {
        let order = OrderSnapshot {
            order_id: "o1".into(),
            instrument_id: "rb2501".into(),
            direction: Direction::Buy,
            offset: Offset::Open,
            price: 3500.0,
            volume: 1,
            market_type: MarketType::CNFutures,
            account_id: "acc1".into(),
        };
        let mut portfolio = PortfolioSnapshot::default();
        portfolio.cash = cash;
        portfolio.total_value = 500_000.0;
        let market = MarketState {
            prices: {
                let mut m = HashMap::new();
                m.insert("rb2501".into(), 3500.0);
                m
            },
            is_trading,
            ..Default::default()
        };
        RiskContext::new(order, portfolio, market)
    }

    #[test]
    fn test_approve_normal() {
        let svc = RiskService::new(MarketType::CNFutures, 500_000.0);
        let ctx = make_ctx(500_000.0, true);
        let decision = svc.evaluate(&ctx);
        assert!(decision.approved, "应该通过：{:?}", decision.block_reasons);
    }

    #[test]
    fn test_block_outside_trading_hours() {
        let svc = RiskService::new(MarketType::CNFutures, 500_000.0);
        let ctx = make_ctx(500_000.0, false);
        let decision = svc.evaluate(&ctx);
        assert!(!decision.approved);
    }

    #[test]
    fn test_kill_switch() {
        let svc = RiskService::new(MarketType::CN, 100_000.0);
        svc.trigger_kill_switch();
        let ctx = make_ctx(100_000.0, true);
        let decision = svc.evaluate(&ctx);
        assert!(!decision.approved);
        assert!(decision.block_reasons.iter().any(|r| r.contains("Kill Switch")));
    }

    #[test]
    fn test_halt_blocks_all() {
        let mut svc = RiskService::new(MarketType::CN, 100_000.0);
        svc.escalate(RiskLevel::Halt, "test", 0);
        let ctx = make_ctx(100_000.0, true);
        let decision = svc.evaluate(&ctx);
        assert!(!decision.approved);
    }

    #[test]
    fn test_restrict_blocks_open() {
        let mut svc = RiskService::new(MarketType::CNFutures, 500_000.0);
        svc.escalate(RiskLevel::Restrict, "high_loss", 0);
        let ctx = make_ctx(500_000.0, true); // 开仓
        let decision = svc.evaluate(&ctx);
        assert!(!decision.approved);
        assert!(decision.block_reasons.iter().any(|r| r.contains("禁止开仓")));
    }

    #[test]
    fn test_update_escalates_on_loss() {
        let mut svc = RiskService::new(MarketType::CN, 100_000.0);
        // 亏损 7%（触发 RESTRICT）
        svc.update(93_000.0, 0.20, 0);
        assert!(svc.risk_level() >= RiskLevel::Restrict);
    }

    #[test]
    fn test_daily_reset() {
        let mut svc = RiskService::new(MarketType::CN, 100_000.0);
        svc.escalate(RiskLevel::Warning, "test", 0);
        svc.daily_reset(86400000);
        assert_eq!(svc.risk_level(), RiskLevel::Normal);
    }
}
