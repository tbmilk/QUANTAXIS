//! 风控配置与 Kill Switch（Phase 5）
//!
//! - `RiskConfig`：TOML 可反序列化的风控参数配置
//! - `KillSwitch`：线程安全的紧急停机开关

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

// ─── Kill Switch ──────────────────────────────────────────────────────────────

/// 线程安全的紧急停机开关
///
/// 一旦触发（`trigger()`），风控服务拒绝一切新订单。
/// 可通过 `share()` 在多线程间共享同一个开关。
#[derive(Clone, Default)]
pub struct KillSwitch {
    triggered: Arc<AtomicBool>,
}

impl KillSwitch {
    pub fn new() -> Self {
        Self { triggered: Arc::new(AtomicBool::new(false)) }
    }

    /// 触发紧急停机
    pub fn trigger(&self) {
        self.triggered.store(true, Ordering::SeqCst);
    }

    /// 解除（仅限运维人员手动操作）
    pub fn reset(&self) {
        self.triggered.store(false, Ordering::SeqCst);
    }

    /// 是否已触发
    pub fn is_triggered(&self) -> bool {
        self.triggered.load(Ordering::SeqCst)
    }

    /// 返回共享同一底层 Arc 的副本
    pub fn share(&self) -> Self {
        Self { triggered: Arc::clone(&self.triggered) }
    }
}

impl std::fmt::Debug for KillSwitch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KillSwitch(triggered={})", self.is_triggered())
    }
}

// ─── 风控配置 ─────────────────────────────────────────────────────────────────

/// 单账户风控参数
#[derive(Debug, Clone)]
pub struct RiskConfig {
    /// 账户初始资金
    pub initial_capital: f64,
    /// 单日最大亏损限额（绝对值，超过则触发 LIQUIDATE）
    pub max_daily_loss: f64,
    /// 单日最大亏损比例（0.10 = 10%，超过则触发 HALT）
    pub max_daily_loss_pct: f64,
    /// 最大杠杆（超过则触发 RESTRICT）
    pub max_leverage: f64,
    /// 单一持仓集中度上限
    pub max_concentration: f64,
    /// 单笔订单最大金额（0.0 = 不限）
    pub max_order_value: f64,
    /// 单日最大成交次数（0 = 不限）
    pub max_trade_count: u32,
    /// 投资组合预测波动率触发 WARNING 的阈值
    pub vol_warn_threshold: f64,
    /// 投资组合预测波动率触发 RESTRICT 的阈值
    pub vol_restrict_threshold: f64,
    /// 是否启用微结构风控（需要 Level2 行情）
    pub enable_microstructure: bool,
    /// 是否启用因子风控
    pub enable_factor_risk: bool,
    /// VaR 置信水平（0.95 = 95%）
    pub var_confidence: f64,
    /// EWMA 衰减系数（0.94 = RiskMetrics 日频标准）
    pub ewma_lambda: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            initial_capital: 1_000_000.0,
            max_daily_loss: 50_000.0,
            max_daily_loss_pct: 0.05,
            max_leverage: 2.0,
            max_concentration: 0.30,
            max_order_value: 0.0,
            max_trade_count: 500,
            vol_warn_threshold: 0.40,
            vol_restrict_threshold: 0.60,
            enable_microstructure: false,
            enable_factor_risk: false,
            var_confidence: 0.95,
            ewma_lambda: 0.94,
        }
    }
}

impl RiskConfig {
    /// A 股默认配置
    pub fn cn_default() -> Self {
        Self {
            initial_capital: 1_000_000.0,
            max_daily_loss: 30_000.0,
            max_daily_loss_pct: 0.03,
            max_leverage: 1.0,
            max_concentration: 0.20,
            max_order_value: 200_000.0,
            max_trade_count: 100,
            ..Default::default()
        }
    }

    /// 期货账户默认配置
    pub fn cn_futures_default() -> Self {
        Self {
            initial_capital: 500_000.0,
            max_daily_loss: 50_000.0,
            max_daily_loss_pct: 0.10,
            max_leverage: 10.0,
            max_concentration: 0.50,
            max_order_value: 0.0,
            max_trade_count: 500,
            vol_warn_threshold: 0.50,
            vol_restrict_threshold: 0.80,
            ..Default::default()
        }
    }

    /// 加密货币配置
    pub fn crypto_default() -> Self {
        Self {
            initial_capital: 100_000.0,
            max_daily_loss: 20_000.0,
            max_daily_loss_pct: 0.20,
            max_leverage: 5.0,
            max_concentration: 0.50,
            max_order_value: 0.0,
            max_trade_count: 2000,
            vol_warn_threshold: 0.80,
            vol_restrict_threshold: 1.50,
            ..Default::default()
        }
    }

    /// 验证配置合法性
    pub fn validate(&self) -> Result<(), String> {
        if self.initial_capital <= 0.0 {
            return Err("initial_capital 必须大于 0".to_string());
        }
        if self.max_daily_loss_pct <= 0.0 || self.max_daily_loss_pct >= 1.0 {
            return Err("max_daily_loss_pct 必须在 (0, 1) 范围内".to_string());
        }
        if self.max_concentration <= 0.0 || self.max_concentration > 1.0 {
            return Err("max_concentration 必须在 (0, 1]".to_string());
        }
        if self.ewma_lambda <= 0.0 || self.ewma_lambda >= 1.0 {
            return Err("ewma_lambda 必须在 (0, 1) 范围内".to_string());
        }
        if self.var_confidence <= 0.0 || self.var_confidence >= 1.0 {
            return Err("var_confidence 必须在 (0, 1) 范围内".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kill_switch_trigger_reset() {
        let ks = KillSwitch::new();
        assert!(!ks.is_triggered());
        ks.trigger();
        assert!(ks.is_triggered());
        ks.reset();
        assert!(!ks.is_triggered());
    }

    #[test]
    fn test_kill_switch_share() {
        let ks1 = KillSwitch::new();
        let ks2 = ks1.share();
        ks1.trigger();
        assert!(ks2.is_triggered()); // 共享同一 Arc
    }

    #[test]
    fn test_risk_config_default_valid() {
        assert!(RiskConfig::default().validate().is_ok());
        assert!(RiskConfig::cn_default().validate().is_ok());
        assert!(RiskConfig::cn_futures_default().validate().is_ok());
    }

    #[test]
    fn test_risk_config_invalid() {
        let mut cfg = RiskConfig::default();
        cfg.initial_capital = -1.0;
        assert!(cfg.validate().is_err());
    }
}
