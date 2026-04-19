//! 风控系统（Quantitative Risk System v1.0）
//!
//! 根据 `test/qaself/quant_risk_system_design_v1.md` 设计实现，分 5 个阶段：
//!
//! ## Phase 1 — 核心风控框架
//! - [`context`]：`RiskContext`，封装订单、持仓、市场、数据、预测等状态
//! - [`rules`]：基于插件/优先级的规则引擎，内置 8 条常用规则
//! - [`statemachine`]：风险状态机 `NORMAL → WARNING → RESTRICT → LIQUIDATE → HALT`
//! - [`service`]：`RiskService` 主入口，串联 Kill Switch → 状态机 → 规则引擎
//!
//! ## Phase 2 — 市场集成与执行层
//! - [`market`]：`MarketProfile`（CN/CNFutures/HK/US/Crypto）
//! - [`execution`]：`BrokerAdapter` Trait + `MockBroker` + `OrderRouter`
//!
//! ## Phase 3 — 高级风险模型
//! - [`budget`]：风险贡献分解、风险平价、目标风险贡献权重、最大分散化
//! - [`factor`]：OLS 因子回归、Beta 敞口、方差分解、因子中性化
//! - [`riskmodes`]：协方差矩阵（cov）、MVO、Ledoit-Wolf 收缩、Black-Litterman
//!
//! ## Phase 4 — 预测与微结构
//! - [`forecast`]：EWMA 波动率、历史 VaR/ES、参数 VaR/ES、市场状态识别
//! - [`microstructure`]：订单簿冲击模拟、滑点估计、流动性检查、TWAP/VWAP 拆单
//!
//! ## Phase 5 — 配置与运维
//! - [`config`]：`RiskConfig`（TOML 友好）、`KillSwitch`（线程安全紧急停机）

pub mod riskmodes;

// Phase 1
pub mod context;
pub mod rules;
pub mod statemachine;
pub mod service;

// Phase 2
pub mod market;
pub mod execution;

// Phase 3
pub mod budget;
pub mod factor;

// Phase 4
pub mod forecast;
pub mod microstructure;

// Phase 5
pub mod config;
pub mod redis_process;

// ─── 常用类型重导出 ────────────────────────────────────────────────────────────

pub use riskmodes::{
    annualized_cov, annualized_returns, black_litterman, correlation, efficient_frontier,
    efficient_return, implied_returns, ledoit_wolf, linear_shrinkage, max_sharpe, min_variance,
    portfolio_std, portfolio_variance, sample_cov, BLInput, BLOutput, Matrix, PortfolioResult,
};

pub use context::{
    DataContext, Direction, ForecastResult, MarketRegime, MarketState, Offset, OrderBook,
    OrderSnapshot, PortfolioSnapshot, PositionSnapshot, RiskContext,
};

pub use market::{MarketProfile, MarketType};

pub use rules::{default_rule_engine, EngineResult, RuleEngine, RuleVerdict};

pub use statemachine::{RiskLevel, RiskStateMachine};

pub use service::{RiskDecision, RiskService};

pub use config::{KillSwitch, RiskConfig};

pub use redis_process::{ControlCommand, RiskEvent, RiskRedisProcess, RiskSnapshot};

pub use execution::{BrokerAdapter, MockBroker, OrderAck, OrderRouter, OrderStatus, TradeReport};

pub use budget::{
    diversification_ratio, max_diversification_weights, percentage_risk_contribution,
    risk_contribution, risk_parity_weights, target_risk_contribution_weights,
};

pub use factor::{FactorRiskEngine, OLSResult, VarianceDecomposition};

pub use forecast::{
    expected_shortfall, historical_var, parametric_es, parametric_var,
    detect_regime, EWMAVolatility, ForecastEngine,
};

pub use microstructure::{linear_impact_price, LiquidityCheck, MicrostructureEngine};
