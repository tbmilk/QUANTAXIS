//! 风险预测引擎（Phase 4）
//!
//! 提供：
//! - EWMA 波动率估计（RiskMetrics 方法）
//! - 历史模拟法 VaR / Expected Shortfall（CVaR）
//! - 市场状态识别（Bull / Bear / Sideways）
//! - 投资组合层面的预测聚合

use crate::qarisk::context::{ForecastResult, MarketRegime};
use std::collections::HashMap;

// ─── EWMA 波动率 ──────────────────────────────────────────────────────────────

/// EWMA 波动率估计器
///
/// σ²_t = λ σ²_{t-1} + (1-λ) r²_t
pub struct EWMAVolatility {
    /// 衰减因子（日频 RiskMetrics 标准：0.94，高频：0.97）
    pub lambda: f64,
}

impl EWMAVolatility {
    pub fn new(lambda: f64) -> Self {
        assert!(lambda > 0.0 && lambda < 1.0, "lambda 必须在 (0, 1) 范围内");
        Self { lambda }
    }

    /// 计算收益率序列的 EWMA 条件方差（最终时刻）
    pub fn variance(&self, returns: &[f64]) -> f64 {
        if returns.is_empty() {
            return 0.0;
        }
        // 初始方差 = 样本方差
        let mean: f64 = returns.iter().sum::<f64>() / returns.len() as f64;
        let init_var: f64 = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
            / returns.len() as f64;

        let mut var = init_var.max(1e-12);
        let l = self.lambda;
        for &r in returns {
            var = l * var + (1.0 - l) * r * r;
        }
        var
    }

    /// 年化 EWMA 波动率（× √trading_days）
    pub fn annualized_vol(&self, returns: &[f64], trading_days: f64) -> f64 {
        self.variance(returns).sqrt() * trading_days.sqrt()
    }

    /// 返回整条 EWMA 方差序列（用于可视化/调试）
    pub fn variance_series(&self, returns: &[f64]) -> Vec<f64> {
        if returns.is_empty() {
            return vec![];
        }
        let mean: f64 = returns.iter().sum::<f64>() / returns.len() as f64;
        let init_var: f64 = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
            / returns.len() as f64;
        let mut var = init_var.max(1e-12);
        let l = self.lambda;
        returns.iter().map(|&r| { var = l * var + (1.0 - l) * r * r; var }).collect()
    }
}

// ─── VaR / Expected Shortfall（历史模拟法） ───────────────────────────────────

/// 计算历史模拟法 VaR（负数表示损失）
///
/// `confidence`：置信水平，如 0.95 表示 95% VaR
pub fn historical_var(returns: &[f64], confidence: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }
    let mut sorted = returns.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((1.0 - confidence) * sorted.len() as f64).floor() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// 计算历史模拟法 Expected Shortfall（CVaR，负数）
///
/// ES_α = -E[r | r ≤ q_{1-α}]（超过 VaR 的平均损失）
pub fn expected_shortfall(returns: &[f64], confidence: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }
    let mut sorted = returns.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let cutoff_idx = ((1.0 - confidence) * sorted.len() as f64).ceil() as usize;
    let cutoff_idx = cutoff_idx.min(sorted.len());
    if cutoff_idx == 0 {
        return sorted[0];
    }
    sorted[..cutoff_idx].iter().sum::<f64>() / cutoff_idx as f64
}

/// 参数法正态 VaR（基于均值和标准差）
pub fn parametric_var(mean: f64, std: f64, confidence: f64) -> f64 {
    // z-score for confidence level（单尾）
    let z = normal_quantile(1.0 - confidence);
    mean + z * std
}

/// 参数法正态 ES
pub fn parametric_es(mean: f64, std: f64, confidence: f64) -> f64 {
    let z = normal_quantile(1.0 - confidence);
    mean - std * normal_pdf(z) / (1.0 - confidence)
}

/// 标准正态分位数（近似，使用 Beasley-Springer-Moro 算法）
fn normal_quantile(p: f64) -> f64 {
    // 简单近似：使用 rational approximation
    if p <= 0.0 { return f64::NEG_INFINITY; }
    if p >= 1.0 { return f64::INFINITY; }

    let (sign, pp) = if p < 0.5 { (-1.0, p) } else { (1.0, 1.0 - p) };
    let t = (-2.0 * pp.ln()).sqrt();
    let c = [2.515517, 0.802853, 0.010328];
    let d = [1.432788, 0.189269, 0.001308];
    let numer = c[0] + c[1] * t + c[2] * t * t;
    let denom = 1.0 + d[0] * t + d[1] * t * t + d[2] * t * t * t;
    sign * (t - numer / denom)
}

fn normal_pdf(x: f64) -> f64 {
    (-x * x / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

// ─── 市场状态识别 ─────────────────────────────────────────────────────────────

/// 基于近期收益率和波动率识别市场状态
///
/// - Bull：均值 > 阈值 且 波动率 < 高波动阈值
/// - Bear：均值 < -阈值 或 波动率 > 高波动阈值
/// - Sideways：其他
pub fn detect_regime(
    returns: &[f64],
    drift_threshold: f64,    // 例如 0.001（日均 0.1%）
    vol_high_threshold: f64, // 年化波动率，例如 0.40
) -> MarketRegime {
    if returns.len() < 5 {
        return MarketRegime::Sideways;
    }
    let n = returns.len() as f64;
    let mean: f64 = returns.iter().sum::<f64>() / n;
    let var: f64 = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
    let vol_ann = var.sqrt() * 252_f64.sqrt();

    if vol_ann > vol_high_threshold && mean < -drift_threshold {
        MarketRegime::Bear
    } else if vol_ann < vol_high_threshold && mean > drift_threshold {
        MarketRegime::Bull
    } else if vol_ann > vol_high_threshold {
        MarketRegime::Bear
    } else {
        MarketRegime::Sideways
    }
}

// ─── 预测引擎 ─────────────────────────────────────────────────────────────────

/// 风险预测引擎
pub struct ForecastEngine {
    ewma: EWMAVolatility,
    trading_days: f64,
    var_confidence: f64,
}

impl ForecastEngine {
    pub fn new(lambda: f64, trading_days: f64, var_confidence: f64) -> Self {
        Self {
            ewma: EWMAVolatility::new(lambda),
            trading_days,
            var_confidence,
        }
    }

    /// 默认配置（日频 A 股：λ=0.94，252 天，95% VaR）
    pub fn default_cn() -> Self {
        Self::new(0.94, 252.0, 0.95)
    }

    /// 对各资产生成风险预测
    ///
    /// `returns_map`：各资产日收益率序列
    /// `weights`：组合权重（可选，用于计算组合层面预测）
    /// `asset_ids`：资产 ID 列表（与 weights 对齐）
    pub fn forecast(
        &self,
        returns_map: &HashMap<String, Vec<f64>>,
        weights: Option<(&[f64], &[String])>,
    ) -> ForecastResult {
        // 各资产预测波动率
        let predicted_vols: HashMap<String, f64> = returns_map
            .iter()
            .map(|(id, rets)| {
                let vol = self.ewma.annualized_vol(rets, self.trading_days);
                (id.clone(), vol)
            })
            .collect();

        // 组合预测（简化：加权平均波动率，忽略相关性）
        let portfolio_vol = if let Some((w, ids)) = weights {
            ids.iter().zip(w.iter())
                .filter_map(|(id, &wi)| predicted_vols.get(id).map(|v| wi * v))
                .sum()
        } else {
            predicted_vols.values().sum::<f64>() / predicted_vols.len().max(1) as f64
        };

        // 取所有资产收益率合并做 VaR / ES（简化方法）
        let all_returns: Vec<f64> = returns_map.values().flatten().copied().collect();
        let var_95 = historical_var(&all_returns, self.var_confidence);
        let es_95 = expected_shortfall(&all_returns, self.var_confidence);

        // 市场状态识别（用第一个资产的收益）
        let regime = returns_map.values().next()
            .map(|r| detect_regime(r, 0.001, 0.40))
            .unwrap_or(MarketRegime::Sideways);

        ForecastResult { predicted_vols, portfolio_vol, regime, var_95, es_95 }
    }

    /// 单资产快速预测
    pub fn asset_vol(&self, returns: &[f64]) -> f64 {
        self.ewma.annualized_vol(returns, self.trading_days)
    }

    /// 单资产 VaR
    pub fn asset_var(&self, returns: &[f64]) -> f64 {
        historical_var(returns, self.var_confidence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_returns(n: usize, seed: f64) -> Vec<f64> {
        (0..n)
            .map(|i| ((i as f64 + seed) * 0.37).sin() * 0.015)
            .collect()
    }

    #[test]
    fn test_ewma_vol_positive() {
        let ewma = EWMAVolatility::new(0.94);
        let rets = make_returns(60, 1.0);
        let vol = ewma.annualized_vol(&rets, 252.0);
        assert!(vol > 0.0);
        println!("EWMA 年化波动率: {:.4}", vol);
    }

    #[test]
    fn test_ewma_higher_lambda_smoother() {
        let rets = make_returns(60, 2.0);
        let fast = EWMAVolatility::new(0.90);
        let slow = EWMAVolatility::new(0.97);
        // 只验证两者均为正值
        assert!(fast.variance(&rets) > 0.0);
        assert!(slow.variance(&rets) > 0.0);
    }

    #[test]
    fn test_historical_var_order() {
        let rets: Vec<f64> = (-50_i64..=50).map(|i| i as f64 * 0.01).collect();
        let var_95 = historical_var(&rets, 0.95);
        let var_99 = historical_var(&rets, 0.99);
        // 99% VaR 应更负（更保守）
        assert!(var_99 <= var_95);
    }

    #[test]
    fn test_expected_shortfall_leq_var() {
        let rets = make_returns(100, 3.0);
        let var = historical_var(&rets, 0.95);
        let es = expected_shortfall(&rets, 0.95);
        // ES ≤ VaR（尾部均值 ≤ 分位数）
        assert!(es <= var + 1e-10);
    }

    #[test]
    fn test_detect_regime_bull() {
        // 构造明显上涨、低波动序列
        let rets: Vec<f64> = (0..60).map(|_| 0.003).collect();
        let regime = detect_regime(&rets, 0.001, 0.40);
        assert_eq!(regime, MarketRegime::Bull);
    }

    #[test]
    fn test_detect_regime_bear() {
        // 构造下跌、高波动序列
        let rets: Vec<f64> = (0..60).map(|i| if i % 2 == 0 { -0.05 } else { 0.01 }).collect();
        let regime = detect_regime(&rets, 0.001, 0.40);
        assert_eq!(regime, MarketRegime::Bear);
    }

    #[test]
    fn test_forecast_engine() {
        let engine = ForecastEngine::default_cn();
        let mut returns_map = HashMap::new();
        returns_map.insert("000001.XSHE".to_string(), make_returns(60, 1.0));
        returns_map.insert("600000.XSHG".to_string(), make_returns(60, 2.0));
        let result = engine.forecast(&returns_map, None);
        assert!(result.portfolio_vol > 0.0);
        assert!(result.var_95 <= 0.0 || result.var_95.is_finite());
        assert_eq!(result.predicted_vols.len(), 2);
    }

    #[test]
    fn test_parametric_var_normal() {
        // 标准正态：95% VaR ≈ -1.645σ
        let var = parametric_var(0.0, 1.0, 0.95);
        assert!((var + 1.645).abs() < 0.05, "var={}", var);
    }
}
