//! 投资组合构建与绩效分析
//!
//! 提供：
//! - 收益率序列计算
//! - 常用绩效指标（Sharpe、Sortino、最大回撤、Calmar）
//! - 等权 / 风险平价权重

// ─── 收益率计算 ───────────────────────────────────────────────────

/// 从价格序列计算简单收益率（日收益率）
pub fn simple_returns(prices: &[f64]) -> Vec<f64> {
    prices.windows(2).map(|w| (w[1] - w[0]) / w[0]).collect()
}

/// 从价格序列计算对数收益率
pub fn log_returns(prices: &[f64]) -> Vec<f64> {
    prices.windows(2).map(|w| (w[1] / w[0]).ln()).collect()
}

/// 计算累积收益率（从初始价格到末尾）
pub fn total_return(prices: &[f64]) -> f64 {
    if prices.is_empty() || prices[0] == 0.0 { return 0.0; }
    (prices[prices.len() - 1] - prices[0]) / prices[0]
}

// ─── 绩效指标 ─────────────────────────────────────────────────────

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn std_dev(v: &[f64]) -> f64 {
    let m = mean(v);
    let var = v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64;
    var.sqrt()
}

/// 年化夏普比率（假设 trading_days 个交易日/年）
pub fn sharpe_ratio(returns: &[f64], risk_free_daily: f64, trading_days: f64) -> f64 {
    if returns.len() < 2 { return 0.0; }
    let excess: Vec<f64> = returns.iter().map(|r| r - risk_free_daily).collect();
    let m = mean(&excess);
    let s = std_dev(&excess);
    if s == 0.0 { return 0.0; }
    m / s * trading_days.sqrt()
}

/// 年化 Sortino 比率（仅用下行标准差）
pub fn sortino_ratio(returns: &[f64], risk_free_daily: f64, trading_days: f64) -> f64 {
    if returns.len() < 2 { return 0.0; }
    let excess: Vec<f64> = returns.iter().map(|r| r - risk_free_daily).collect();
    let m = mean(&excess);
    let downside: Vec<f64> = excess.iter().filter(|&&x| x < 0.0).copied().collect();
    if downside.is_empty() { return f64::INFINITY; }
    let downside_std = {
        let var = downside.iter().map(|x| x * x).sum::<f64>() / downside.len() as f64;
        var.sqrt()
    };
    if downside_std == 0.0 { return 0.0; }
    m / downside_std * trading_days.sqrt()
}

/// 最大回撤（峰值到谷值的最大跌幅，返回负数）
pub fn max_drawdown(prices: &[f64]) -> f64 {
    let mut peak = f64::NEG_INFINITY;
    let mut max_dd = 0.0f64;
    for &p in prices {
        if p > peak { peak = p; }
        let dd = (p - peak) / peak;
        if dd < max_dd { max_dd = dd; }
    }
    max_dd
}

/// Calmar 比率（年化收益 / |最大回撤|）
pub fn calmar_ratio(prices: &[f64], trading_days: f64) -> f64 {
    if prices.len() < 2 { return 0.0; }
    let n_days = (prices.len() - 1) as f64;
    let annual_ret = total_return(prices) * (trading_days / n_days);
    let mdd = max_drawdown(prices).abs();
    if mdd == 0.0 { return f64::INFINITY; }
    annual_ret / mdd
}

/// 年化波动率
pub fn annualized_volatility(returns: &[f64], trading_days: f64) -> f64 {
    if returns.len() < 2 { return 0.0; }
    std_dev(returns) * trading_days.sqrt()
}

// ─── 权重构建 ─────────────────────────────────────────────────────

/// 等权权重
pub fn equal_weights(n: usize) -> Vec<f64> {
    vec![1.0 / n as f64; n]
}

/// 风险平价权重（逆波动率加权）
///
/// `vols`：各资产的波动率（或标准差）
pub fn risk_parity_weights(vols: &[f64]) -> Vec<f64> {
    let inv: Vec<f64> = vols.iter().map(|v| if *v > 0.0 { 1.0 / v } else { 0.0 }).collect();
    let total: f64 = inv.iter().sum();
    if total == 0.0 {
        return equal_weights(vols.len());
    }
    inv.iter().map(|x| x / total).collect()
}

/// 市值加权权重
pub fn cap_weights(market_caps: &[f64]) -> Vec<f64> {
    let total: f64 = market_caps.iter().sum();
    if total == 0.0 { return equal_weights(market_caps.len()); }
    market_caps.iter().map(|x| x / total).collect()
}

// ─── 投资组合综合评价 ─────────────────────────────────────────────

/// 投资组合绩效摘要
#[derive(Debug, Clone)]
pub struct PerformanceSummary {
    pub total_return: f64,
    pub annualized_return: f64,
    pub annualized_volatility: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub max_drawdown: f64,
    pub calmar_ratio: f64,
}

/// 计算投资组合绩效摘要
///
/// `prices`：净值序列，`risk_free_annual`：年化无风险利率，
/// `trading_days`：每年交易日数（股票 252，期货 245）
pub fn performance_summary(
    prices: &[f64],
    risk_free_annual: f64,
    trading_days: f64,
) -> PerformanceSummary {
    let rets = simple_returns(prices);
    let n_days = (prices.len() - 1) as f64;
    let rfr_daily = risk_free_annual / trading_days;

    let tr = total_return(prices);
    let ann_ret = tr * (trading_days / n_days);
    let ann_vol = annualized_volatility(&rets, trading_days);
    let sr = sharpe_ratio(&rets, rfr_daily, trading_days);
    let sor = sortino_ratio(&rets, rfr_daily, trading_days);
    let mdd = max_drawdown(prices);
    let calmar = calmar_ratio(prices, trading_days);

    PerformanceSummary {
        total_return: tr,
        annualized_return: ann_ret,
        annualized_volatility: ann_vol,
        sharpe_ratio: sr,
        sortino_ratio: sor,
        max_drawdown: mdd,
        calmar_ratio: calmar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nav() -> Vec<f64> {
        vec![1.0, 1.02, 1.01, 1.05, 1.03, 1.08, 1.06, 1.10, 1.07, 1.12]
    }

    #[test]
    fn test_total_return() {
        let nav = nav();
        let tr = total_return(&nav);
        assert!((tr - 0.12).abs() < 1e-10);
    }

    #[test]
    fn test_max_drawdown_negative() {
        let mdd = max_drawdown(&nav());
        assert!(mdd < 0.0);
    }

    #[test]
    fn test_sharpe_positive() {
        let rets = simple_returns(&nav());
        let sr = sharpe_ratio(&rets, 0.0, 252.0);
        assert!(sr > 0.0);
    }

    #[test]
    fn test_equal_weights_sum() {
        let w = equal_weights(5);
        assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_risk_parity_sum() {
        let vols = vec![0.1, 0.2, 0.15];
        let w = risk_parity_weights(&vols);
        assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }
}
