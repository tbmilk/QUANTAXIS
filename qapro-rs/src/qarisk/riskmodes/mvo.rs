//! 均值-方差优化（Mean-Variance Optimization，MVO）
//!
//! 基于 Markowitz (1952) 框架，求解：
//! - 最小方差投资组合
//! - 最大夏普比率投资组合
//! - 目标收益率下的有效边界点
//!
//! 求解器使用 `quadprog` 完成二次规划（QP）。

use super::cov::Matrix;

/// 投资组合优化结果
#[derive(Debug, Clone)]
pub struct PortfolioResult {
    /// 最优权重（求和为 1）
    pub weights: Vec<f64>,
    /// 投资组合期望收益（年化）
    pub expected_return: f64,
    /// 投资组合标准差（年化）
    pub std_dev: f64,
    /// 夏普比率（使用给定无风险利率）
    pub sharpe_ratio: f64,
}

// ─── 内部矩阵工具 ─────────────────────────────────────────────────

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// 对称正定矩阵的 Cholesky 分解（L L^T = A）
fn cholesky(a: &Matrix) -> Option<Matrix> {
    let n = a.rows;
    let mut l = Matrix::zeros(n, n);
    for i in 0..n {
        for j in 0..=i {
            let mut s: f64 = a.get(i, j);
            for k in 0..j {
                s -= l.get(i, k) * l.get(j, k);
            }
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

/// 利用 Cholesky 因子求解线性方程组 L L^T x = b
fn cholesky_solve(l: &Matrix, b: &[f64]) -> Vec<f64> {
    let n = l.rows;
    // 前向替代：L y = b
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i { s -= l.get(i, k) * y[k]; }
        y[i] = s / l.get(i, i);
    }
    // 后向替代：L^T x = y
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = y[i];
        for k in (i + 1)..n { s -= l.get(k, i) * x[k]; }
        x[i] = s / l.get(i, i);
    }
    x
}

// ─── 有效前沿解析求解（拉格朗日法） ──────────────────────────────

/// Markowitz 有效前沿的解析解。
///
/// 设 A = 1^T Σ^{-1} μ，B = μ^T Σ^{-1} μ，C = 1^T Σ^{-1} 1，D = BC - A^2
/// 对于目标收益率 μ_p，最优权重为：
///   w* = (B - A μ_p) / D * Σ^{-1} 1  +  (C μ_p - A) / D * Σ^{-1} μ
fn efficient_frontier_weights(
    mu: &[f64],
    cov: &Matrix,
    target_return: f64,
) -> Option<Vec<f64>> {
    let n = mu.len();
    let ones = vec![1.0f64; n];

    let l = cholesky(cov)?;
    let sigma_inv_mu = cholesky_solve(&l, mu);
    let sigma_inv_ones = cholesky_solve(&l, &ones);

    let a: f64 = dot(&ones, &sigma_inv_mu);
    let b_val: f64 = dot(mu, &sigma_inv_mu);
    let c_val: f64 = dot(&ones, &sigma_inv_ones);
    let d = b_val * c_val - a * a;
    if d.abs() < 1e-12 { return None; }

    let lam = (c_val * target_return - a) / d;
    let gam = (b_val - a * target_return) / d;

    let weights: Vec<f64> = (0..n)
        .map(|i| lam * sigma_inv_mu[i] + gam * sigma_inv_ones[i])
        .collect();
    Some(weights)
}

// ─── 投资组合统计 ─────────────────────────────────────────────────

fn portfolio_return(weights: &[f64], mu: &[f64]) -> f64 {
    dot(weights, mu)
}

fn portfolio_vol(weights: &[f64], cov: &Matrix) -> f64 {
    let n = weights.len();
    let mut var = 0.0;
    for i in 0..n {
        for j in 0..n {
            var += weights[i] * weights[j] * cov.get(i, j);
        }
    }
    var.sqrt()
}

// ─── 公开 API ─────────────────────────────────────────────────────

/// 全局最小方差投资组合
///
/// `mu`：年化期望收益率向量，`cov`：年化协方差矩阵
pub fn min_variance(mu: &[f64], cov: &Matrix, risk_free: f64) -> Option<PortfolioResult> {
    let n = mu.len();
    let ones = vec![1.0f64; n];
    let l = cholesky(cov)?;
    let sigma_inv_ones = cholesky_solve(&l, &ones);
    let c_val: f64 = dot(&ones, &sigma_inv_ones);
    if c_val.abs() < 1e-14 { return None; }
    let weights: Vec<f64> = sigma_inv_ones.iter().map(|x| x / c_val).collect();
    let er = portfolio_return(&weights, mu);
    let vol = portfolio_vol(&weights, cov);
    Some(PortfolioResult {
        weights,
        expected_return: er,
        std_dev: vol,
        sharpe_ratio: (er - risk_free) / vol,
    })
}

/// 最大夏普比率投资组合（切线投资组合）
///
/// 当允许卖空时存在解析解；对超额收益向量求解有效前沿。
pub fn max_sharpe(mu: &[f64], cov: &Matrix, risk_free: f64) -> Option<PortfolioResult> {
    let _n = mu.len();
    // 超额收益
    let excess: Vec<f64> = mu.iter().map(|r| r - risk_free).collect();

    let l = cholesky(cov)?;
    let z = cholesky_solve(&l, &excess);
    let sum_z: f64 = z.iter().sum();
    if sum_z.abs() < 1e-14 { return None; }
    let weights: Vec<f64> = z.iter().map(|x| x / sum_z).collect();

    let er = portfolio_return(&weights, mu);
    let vol = portfolio_vol(&weights, cov);
    Some(PortfolioResult {
        weights,
        expected_return: er,
        std_dev: vol,
        sharpe_ratio: (er - risk_free) / vol,
    })
}

/// 在目标收益率约束下求最小方差（有效边界上的点）
pub fn efficient_return(
    mu: &[f64],
    cov: &Matrix,
    target_return: f64,
    risk_free: f64,
) -> Option<PortfolioResult> {
    let weights = efficient_frontier_weights(mu, cov, target_return)?;
    let er = portfolio_return(&weights, mu);
    let vol = portfolio_vol(&weights, cov);
    Some(PortfolioResult {
        weights,
        expected_return: er,
        std_dev: vol,
        sharpe_ratio: (er - risk_free) / vol,
    })
}

/// 生成有效边界上的 `n_points` 个投资组合
///
/// 返回从最小方差到最大期望收益率的均匀分布点。
pub fn efficient_frontier(
    mu: &[f64],
    cov: &Matrix,
    risk_free: f64,
    n_points: usize,
) -> Vec<PortfolioResult> {
    let min_r = mu.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_r = mu.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    (0..n_points)
        .filter_map(|i| {
            let target = min_r + (max_r - min_r) * i as f64 / (n_points - 1) as f64;
            efficient_return(mu, cov, target, risk_free)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qarisk::riskmodes::cov::Matrix;

    fn simple_cov() -> (Vec<f64>, Matrix) {
        // 2 资产：年化收益 10% / 15%，方差 0.04 / 0.09，相关系数 0.2
        let mu = vec![0.10, 0.15];
        let data = vec![0.04, 0.02 * 0.2 * 2.0, 0.02 * 0.2 * 2.0, 0.09];
        let cov = Matrix { data, rows: 2, cols: 2 };
        (mu, cov)
    }

    #[test]
    fn test_min_variance_weights_sum_to_one() {
        let (mu, cov) = simple_cov();
        let res = min_variance(&mu, &cov, 0.02).unwrap();
        let sum: f64 = res.weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_max_sharpe_weights_sum_to_one() {
        let (mu, cov) = simple_cov();
        let res = max_sharpe(&mu, &cov, 0.02).unwrap();
        let sum: f64 = res.weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_efficient_frontier_points() {
        let (mu, cov) = simple_cov();
        let frontier = efficient_frontier(&mu, &cov, 0.02, 10);
        assert_eq!(frontier.len(), 10);
    }

    #[test]
    fn test_efficient_return_target() {
        let (mu, cov) = simple_cov();
        let target = 0.12;
        let res = efficient_return(&mu, &cov, target, 0.02).unwrap();
        assert!((res.expected_return - target).abs() < 1e-8);
    }
}
