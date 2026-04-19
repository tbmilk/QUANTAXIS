//! 风险预算引擎（Phase 3）
//!
//! 提供：
//! - 风险贡献分解（Risk Contribution）
//! - 风险平价权重优化（Risk Parity via iterative approach）
//! - 最大分散化投资组合（Maximum Diversification Portfolio）
//! - 风险预算分配（Target Risk Contribution）

use crate::qarisk::riskmodes::cov::{Matrix, portfolio_variance};

// ─── 风险贡献 ─────────────────────────────────────────────────────────────────

/// 计算各资产的风险贡献（Euler 分解）
///
/// RC_i = w_i × (Σw)_i / σ_p
///
/// 返回：各资产绝对风险贡献（sum = σ_p）
pub fn risk_contribution(weights: &[f64], cov: &Matrix) -> Vec<f64> {
    let n = weights.len();
    assert_eq!(n, cov.rows);

    // Σw（协方差矩阵与权重向量的乘积）
    let mut sigma_w = vec![0.0; n];
    for i in 0..n {
        for j in 0..n {
            sigma_w[i] += cov.get(i, j) * weights[j];
        }
    }

    let portfolio_vol = portfolio_variance(weights, cov).sqrt();
    if portfolio_vol < 1e-14 {
        return vec![0.0; n];
    }

    weights.iter().zip(&sigma_w)
        .map(|(w, sw)| w * sw / portfolio_vol)
        .collect()
}

/// 各资产风险贡献占比（percentage risk contribution，sum = 1）
pub fn percentage_risk_contribution(weights: &[f64], cov: &Matrix) -> Vec<f64> {
    let rc = risk_contribution(weights, cov);
    let total: f64 = rc.iter().sum();
    if total.abs() < 1e-14 {
        return vec![1.0 / weights.len() as f64; weights.len()];
    }
    rc.iter().map(|x| x / total).collect()
}

// ─── 风险平价优化 ─────────────────────────────────────────────────────────────

/// 风险平价权重优化（等风险贡献，CCD 迭代法）
///
/// 目标：使所有资产风险贡献相等，即 RC_i = σ_p / n
///
/// 参考：Maillard, Roncalli, Teïletche (2010)
/// 使用循环坐标下降（Cyclical Coordinate Descent）近似求解。
pub fn risk_parity_weights(cov: &Matrix, max_iter: usize, tol: f64) -> Vec<f64> {
    let n = cov.rows;
    let target_rc = 1.0 / n as f64; // 等风险贡献

    // 初始权重：逆波动率加权
    let vols: Vec<f64> = (0..n).map(|i| cov.get(i, i).sqrt()).collect();
    let inv_vol_sum: f64 = vols.iter().map(|v| if *v > 0.0 { 1.0 / v } else { 0.0 }).sum();
    let mut w: Vec<f64> = vols.iter()
        .map(|v| if *v > 0.0 { 1.0 / (v * inv_vol_sum) } else { 0.0 })
        .collect();

    for _ in 0..max_iter {
        let old_w = w.clone();
        let pv2 = portfolio_variance(&w, cov).max(1e-28);
        // 对每个资产进行乘法更新（Roncalli 2013 标准方法）
        for i in 0..n {
            let mut sigma_w_i = 0.0;
            for j in 0..n { sigma_w_i += cov.get(i, j) * w[j]; }
            // 当前百分比风险贡献
            let prc_i = w[i] * sigma_w_i / pv2;
            if prc_i > 1e-14 {
                // 开方乘法更新：收敛更稳定
                w[i] *= (target_rc / prc_i).sqrt();
                w[i] = w[i].max(1e-8);
            }
        }
        // 归一化
        let wsum: f64 = w.iter().sum();
        if wsum > 1e-14 { for wi in &mut w { *wi /= wsum; } }
        // 收敛判断
        let diff: f64 = w.iter().zip(&old_w).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt();
        if diff < tol { break; }
    }
    w
}

// ─── 目标风险贡献 ─────────────────────────────────────────────────────────────

/// 目标风险贡献权重优化
///
/// `target_rc`：各资产目标风险贡献占比（sum = 1）
/// 迭代策略与风险平价相同，但目标各异。
pub fn target_risk_contribution_weights(
    cov: &Matrix,
    target_rc: &[f64],
    max_iter: usize,
    tol: f64,
) -> Vec<f64> {
    let n = cov.rows;
    assert_eq!(n, target_rc.len());
    let target_sum: f64 = target_rc.iter().sum();
    assert!((target_sum - 1.0).abs() < 1e-6, "target_rc 必须和为 1");

    // 初始权重
    let vols: Vec<f64> = (0..n).map(|i| cov.get(i, i).sqrt()).collect();
    let inv_vol_sum: f64 = vols.iter().map(|v| if *v > 0.0 { 1.0 / v } else { 0.0 }).sum();
    let mut w: Vec<f64> = vols.iter()
        .map(|v| if *v > 0.0 { 1.0 / (v * inv_vol_sum) } else { 0.0 })
        .collect();

    for _ in 0..max_iter {
        let old_w = w.clone();
        let pv2 = portfolio_variance(&w, cov).max(1e-28);
        for i in 0..n {
            let mut sigma_w_i = 0.0;
            for j in 0..n { sigma_w_i += cov.get(i, j) * w[j]; }
            let prc_i = w[i] * sigma_w_i / pv2;
            if prc_i > 1e-14 {
                w[i] *= (target_rc[i] / prc_i).sqrt();
                w[i] = w[i].max(1e-8);
            }
        }
        let wsum: f64 = w.iter().sum();
        if wsum > 1e-14 { for wi in &mut w { *wi /= wsum; } }
        let diff: f64 = w.iter().zip(&old_w).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt();
        if diff < tol { break; }
    }
    w
}

// ─── 最大分散化组合 ───────────────────────────────────────────────────────────

/// 最大分散化组合（Maximum Diversification Portfolio）
///
/// 最大化分散化比率 DR = Σ(w_i × σ_i) / σ_p
/// 等价于最大化 Σ(w_i/σ_i) / σ_p 的对偶问题（Choueifaty & Coignard 2008）
pub fn max_diversification_weights(cov: &Matrix) -> Option<Vec<f64>> {
    let n = cov.rows;
    let vols: Vec<f64> = (0..n).map(|i| cov.get(i, i).sqrt()).collect();

    // 构造相关系数矩阵的逆并求解
    // MDP 权重 ∝ Corr^{-1} × σ（逆相关矩阵 × 波动率向量）
    use crate::qarisk::riskmodes::cov::correlation;
    let corr = correlation(cov);

    // Cholesky 分解相关矩阵
    let l = cholesky_lower(&corr)?;
    let w_unscaled = cholesky_solve_vec(&l, &vols);
    let wsum: f64 = w_unscaled.iter().sum();
    if wsum.abs() < 1e-14 { return None; }

    let w: Vec<f64> = w_unscaled.iter().map(|x| x / wsum).collect();
    Some(w)
}

/// 分散化比率（Diversification Ratio）：DR = Σ(w_i σ_i) / σ_p
pub fn diversification_ratio(weights: &[f64], cov: &Matrix) -> f64 {
    let n = weights.len();
    let vols: Vec<f64> = (0..n).map(|i| cov.get(i, i).sqrt()).collect();
    let weighted_vol_sum: f64 = weights.iter().zip(&vols).map(|(w, v)| w * v).sum();
    let portfolio_vol = portfolio_variance(weights, cov).sqrt();
    if portfolio_vol < 1e-14 { 0.0 } else { weighted_vol_sum / portfolio_vol }
}

// ─── 内部工具（Cholesky） ─────────────────────────────────────────────────────

fn cholesky_lower(a: &Matrix) -> Option<Matrix> {
    let n = a.rows;
    let mut l = Matrix::zeros(n, n);
    for i in 0..n {
        for j in 0..=i {
            let mut s = a.get(i, j);
            for k in 0..j { s -= l.get(i, k) * l.get(j, k); }
            if i == j {
                if s < 0.0 { return None; }
                l.set(i, j, s.sqrt());
            } else {
                let ljj = l.get(j, j);
                if ljj.abs() < 1e-14 { return None; }
                l.set(i, j, s / ljj);
            }
        }
    }
    Some(l)
}

fn cholesky_solve_vec(l: &Matrix, b: &[f64]) -> Vec<f64> {
    let n = l.rows;
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i { s -= l.get(i, k) * y[k]; }
        y[i] = s / l.get(i, i);
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = y[i];
        for k in (i + 1)..n { s -= l.get(k, i) * x[k]; }
        x[i] = s / l.get(i, i);
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qarisk::riskmodes::cov::Matrix;

    fn two_asset_cov() -> (Vec<f64>, Matrix) {
        // 2 资产：vol=0.2 / 0.3，相关系数 0.3
        let cov = Matrix {
            data: vec![0.04, 0.018, 0.018, 0.09],
            rows: 2, cols: 2,
        };
        let w = vec![0.5, 0.5];
        (w, cov)
    }

    #[test]
    fn test_risk_contribution_sums_to_portfolio_vol() {
        let (w, cov) = two_asset_cov();
        let rc = risk_contribution(&w, &cov);
        let total_rc: f64 = rc.iter().sum();
        let pv = portfolio_variance(&w, &cov).sqrt();
        assert!((total_rc - pv).abs() < 1e-10);
    }

    #[test]
    fn test_percentage_rc_sums_to_one() {
        let (w, cov) = two_asset_cov();
        let prc = percentage_risk_contribution(&w, &cov);
        let sum: f64 = prc.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_risk_parity_equal_rc() {
        let (_, cov) = two_asset_cov();
        let w = risk_parity_weights(&cov, 500, 1e-8);
        let prc = percentage_risk_contribution(&w, &cov);
        // 两个资产风险贡献应各约 50%
        assert!((prc[0] - 0.5).abs() < 1e-4, "prc[0]={}", prc[0]);
        assert!((prc[1] - 0.5).abs() < 1e-4, "prc[1]={}", prc[1]);
    }

    #[test]
    fn test_target_risk_contribution() {
        let (_, cov) = two_asset_cov();
        let target = vec![0.3, 0.7];
        let w = target_risk_contribution_weights(&cov, &target, 500, 1e-8);
        let prc = percentage_risk_contribution(&w, &cov);
        assert!((prc[0] - 0.3).abs() < 0.01, "prc[0]={}", prc[0]);
        assert!((prc[1] - 0.7).abs() < 0.01, "prc[1]={}", prc[1]);
    }

    #[test]
    fn test_max_diversification_weights_sum_to_one() {
        let (_, cov) = two_asset_cov();
        let w = max_diversification_weights(&cov).unwrap();
        let sum: f64 = w.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_diversification_ratio_geq_one() {
        let (w, cov) = two_asset_cov();
        let dr = diversification_ratio(&w, &cov);
        assert!(dr >= 1.0);
    }
}
