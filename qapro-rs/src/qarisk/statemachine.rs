//! 风险状态机（Phase 1）
//!
//! 状态流转：NORMAL → WARNING → RESTRICT → LIQUIDATE → HALT
//!
//! - NORMAL    ：正常交易
//! - WARNING   ：风险预警，记录日志并提示，仍允许交易
//! - RESTRICT  ：限制交易，仅允许减仓/平仓
//! - LIQUIDATE ：强制平仓，禁止开仓，系统主动平仓
//! - HALT      ：完全停机，所有订单拒绝

/// 风险等级（状态）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskLevel {
    Normal = 0,
    Warning = 1,
    Restrict = 2,
    Liquidate = 3,
    Halt = 4,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            RiskLevel::Normal => "NORMAL",
            RiskLevel::Warning => "WARNING",
            RiskLevel::Restrict => "RESTRICT",
            RiskLevel::Liquidate => "LIQUIDATE",
            RiskLevel::Halt => "HALT",
        };
        write!(f, "{}", s)
    }
}

/// 状态迁移事件
#[derive(Debug, Clone)]
pub struct StateTransition {
    pub from: RiskLevel,
    pub to: RiskLevel,
    pub trigger: String,
    pub timestamp_ms: i64,
}

/// 风险状态机
///
/// 状态单调上升（风险只会升高不会自动降低），降级需调用 `downgrade`。
pub struct RiskStateMachine {
    current: RiskLevel,
    history: Vec<StateTransition>,
}

impl Default for RiskStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl RiskStateMachine {
    pub fn new() -> Self {
        Self {
            current: RiskLevel::Normal,
            history: Vec::new(),
        }
    }

    pub fn current_level(&self) -> RiskLevel {
        self.current
    }

    /// 尝试将状态提升到 `target`（只升不降）。
    /// 若 `target <= current`，不执行任何操作。
    pub fn escalate(&mut self, target: RiskLevel, trigger: &str, timestamp_ms: i64) -> bool {
        if target <= self.current {
            return false;
        }
        self.history.push(StateTransition {
            from: self.current,
            to: target,
            trigger: trigger.to_string(),
            timestamp_ms,
        });
        self.current = target;
        true
    }

    /// 手动降级（需要操作员确认，不建议自动调用）
    pub fn downgrade(&mut self, target: RiskLevel, reason: &str, timestamp_ms: i64) -> bool {
        if target >= self.current {
            return false;
        }
        self.history.push(StateTransition {
            from: self.current,
            to: target,
            trigger: format!("[MANUAL_DOWNGRADE] {}", reason),
            timestamp_ms,
        });
        self.current = target;
        true
    }

    /// 重置到 NORMAL（用于日初清零）
    pub fn reset(&mut self, timestamp_ms: i64) {
        if self.current != RiskLevel::Normal {
            self.history.push(StateTransition {
                from: self.current,
                to: RiskLevel::Normal,
                trigger: "DAILY_RESET".to_string(),
                timestamp_ms,
            });
            self.current = RiskLevel::Normal;
        }
    }

    /// 是否允许新开仓
    pub fn can_open(&self) -> bool {
        self.current <= RiskLevel::Warning
    }

    /// 是否允许任何交易（含平仓）
    pub fn can_trade(&self) -> bool {
        self.current < RiskLevel::Halt
    }

    /// 是否必须强平
    pub fn must_liquidate(&self) -> bool {
        self.current >= RiskLevel::Liquidate
    }

    /// 迁移历史
    pub fn history(&self) -> &[StateTransition] {
        &self.history
    }

    /// 根据预测波动率和当日亏损率自动评估并触发状态升级
    ///
    /// - pnl_ratio：当日 PnL / 初始资产（负数表示亏损）
    /// - portfolio_vol：投资组合预测年化波动率
    pub fn auto_evaluate(
        &mut self,
        pnl_ratio: f64,
        portfolio_vol: f64,
        timestamp_ms: i64,
    ) {
        // 根据亏损率确定目标风险等级
        let level_by_pnl = if pnl_ratio < -0.20 {
            RiskLevel::Halt
        } else if pnl_ratio < -0.10 {
            RiskLevel::Liquidate
        } else if pnl_ratio < -0.05 {
            RiskLevel::Restrict
        } else if pnl_ratio < -0.02 {
            RiskLevel::Warning
        } else {
            RiskLevel::Normal
        };

        // 根据波动率确定目标风险等级
        let level_by_vol = if portfolio_vol > 0.60 {
            RiskLevel::Restrict
        } else if portfolio_vol > 0.40 {
            RiskLevel::Warning
        } else {
            RiskLevel::Normal
        };

        let target = level_by_pnl.max(level_by_vol);
        if target > self.current {
            let trigger = format!(
                "auto_evaluate: pnl_ratio={:.3}, vol={:.3}",
                pnl_ratio, portfolio_vol
            );
            self.escalate(target, &trigger, timestamp_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sm = RiskStateMachine::new();
        assert_eq!(sm.current_level(), RiskLevel::Normal);
        assert!(sm.can_open());
        assert!(sm.can_trade());
        assert!(!sm.must_liquidate());
    }

    #[test]
    fn test_escalate() {
        let mut sm = RiskStateMachine::new();
        assert!(sm.escalate(RiskLevel::Warning, "test", 1000));
        assert_eq!(sm.current_level(), RiskLevel::Warning);
        // 不能降
        assert!(!sm.escalate(RiskLevel::Normal, "test", 2000));
        assert_eq!(sm.current_level(), RiskLevel::Warning);
    }

    #[test]
    fn test_restrict_blocks_open() {
        let mut sm = RiskStateMachine::new();
        sm.escalate(RiskLevel::Restrict, "high_vol", 0);
        assert!(!sm.can_open());
        assert!(sm.can_trade()); // 平仓仍允许
    }

    #[test]
    fn test_halt_blocks_all() {
        let mut sm = RiskStateMachine::new();
        sm.escalate(RiskLevel::Halt, "kill_switch", 0);
        assert!(!sm.can_open());
        assert!(!sm.can_trade());
    }

    #[test]
    fn test_manual_downgrade() {
        let mut sm = RiskStateMachine::new();
        sm.escalate(RiskLevel::Liquidate, "loss", 0);
        sm.downgrade(RiskLevel::Warning, "risk_cleared", 1000);
        assert_eq!(sm.current_level(), RiskLevel::Warning);
        assert_eq!(sm.history().len(), 2);
    }

    #[test]
    fn test_auto_evaluate_loss() {
        let mut sm = RiskStateMachine::new();
        sm.auto_evaluate(-0.07, 0.20, 0);
        assert_eq!(sm.current_level(), RiskLevel::Restrict);
    }

    #[test]
    fn test_auto_evaluate_high_vol() {
        let mut sm = RiskStateMachine::new();
        sm.auto_evaluate(-0.01, 0.50, 0);
        assert_eq!(sm.current_level(), RiskLevel::Warning);
    }

    #[test]
    fn test_reset() {
        let mut sm = RiskStateMachine::new();
        sm.escalate(RiskLevel::Warning, "test", 0);
        sm.reset(86400000);
        assert_eq!(sm.current_level(), RiskLevel::Normal);
        // 历史保留
        assert_eq!(sm.history().len(), 2);
    }
}
