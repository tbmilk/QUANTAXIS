//! Ledoit-Wolf 协方差矩阵收缩估计
//!
//! 将样本协方差矩阵向结构化目标（单位缩放矩阵）收缩，
//! 以减少小样本下的估计误差。
//!
//! 参考：Ledoit & Wolf (2004) "A well-conditioned estimator for large-dimensional
//! covariance matrices", Journal of Multivariate Analysis.

use super::cov::{sample_cov, Matrix};

/// Ledoit-Wolf Oracle 近似收缩（OAS）
///
/// 计算最优收缩系数 δ* 并返回收缩后的协方差矩阵：
///   Σ_shrunk = (1 - δ) * S + δ * μ * I
///
/// `returns`：T×N 收益率矩阵（T 时间步，N 资产数）
pub fn ledoit_wolf(returns: &Matrix) -> (Matrix, f64) {
    let t = returns.rows as f64;
    let n = returns.cols;

    let s = sample_cov(returns);

    // μ：目标矩阵缩放因子 = trace(S) / n
    let trace_s: f64 = (0..n).map(|i| s.get(i, i)).sum();
    let mu = trace_s / n as f64;

    // δ*（解析公式）
    // delta = ( sum_i sum_j s_ij^2  +  trace(S)^2 )
    //         / ( (T+1-2/N) * (sum_i sum_j s_ij^2 - trace(S^2)/N) )
    let sum_sq: f64 = s.data.iter().map(|x| x * x).sum();
    let trace_s2: f64 = {
        let s2 = s.matmul(&s);
        (0..n).map(|i| s2.get(i, i)).sum()
    };

    let numerator = sum_sq + trace_s * trace_s;
    let denominator = (t + 1.0 - 2.0 / n as f64) * (sum_sq - trace_s2 / n as f64);

    let delta = if denominator.abs() < 1e-14 {
        1.0_f64.min(0.0_f64.max(0.0)) // fallback: no shrinkage
    } else {
        (numerator / denominator / t).min(1.0).max(0.0)
    };

    // Σ_shrunk = (1 - δ) * S + δ * μ * I
    let mut shrunk = s.scale(1.0 - delta);
    for i in 0..n {
        let v = shrunk.get(i, i) + delta * mu;
        shrunk.set(i, i, v);
    }

    (shrunk, delta)
}

/// 线性收缩（手动指定收缩系数）
///
/// `alpha`：收缩强度，范围 [0, 1]。0 = 纯样本协方差，1 = 纯单位矩阵（缩放）
pub fn linear_shrinkage(cov: &Matrix, alpha: f64) -> Matrix {
    assert!((0.0..=1.0).contains(&alpha), "alpha must be in [0, 1]");
    let n = cov.rows;
    let trace: f64 = (0..n).map(|i| cov.get(i, i)).sum();
    let mu = trace / n as f64;
    let mut out = cov.scale(1.0 - alpha);
    for i in 0..n {
        let v = out.get(i, i) + alpha * mu;
        out.set(i, i, v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qarisk::riskmodes::cov::Matrix;

    fn make_returns(t: usize, n: usize, seed_offset: f64) -> Matrix {
        let data: Vec<f64> = (0..t * n)
            .map(|i| ((i as f64 + seed_offset) * 0.003).sin() * 0.01)
            .collect();
        Matrix { data, rows: t, cols: n }
    }

    #[test]
    fn test_ledoit_wolf_positive_definite() {
        let r = make_returns(60, 5, 1.0);
        let (shrunk, delta) = ledoit_wolf(&r);
        assert!(delta >= 0.0 && delta <= 1.0);
        // 对角元素应为正
        for i in 0..5 {
            assert!(shrunk.get(i, i) > 0.0, "diagonal must be positive");
        }
    }

    #[test]
    fn test_linear_shrinkage_alpha_zero() {
        let r = make_returns(30, 3, 2.0);
        let cov = super::super::cov::sample_cov(&r);
        let out = linear_shrinkage(&cov, 0.0);
        for i in 0..3 {
            for j in 0..3 {
                assert!((out.get(i, j) - cov.get(i, j)).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn test_linear_shrinkage_alpha_one() {
        let r = make_returns(30, 3, 3.0);
        let cov = super::super::cov::sample_cov(&r);
        let out = linear_shrinkage(&cov, 1.0);
        let trace: f64 = (0..3).map(|i| cov.get(i, i)).sum();
        let mu = trace / 3.0;
        // 非对角应为 0
        assert!((out.get(0, 1)).abs() < 1e-12);
        // 对角应为 mu
        assert!((out.get(0, 0) - mu).abs() < 1e-12);
    }
}
