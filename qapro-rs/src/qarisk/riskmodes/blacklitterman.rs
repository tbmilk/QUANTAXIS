//! Black-Litterman 模型
//!
//! 将市场均衡收益率（CAPM 隐含）与投资者观点（P, Q, Ω）融合，
//! 得到后验期望收益率，再代入 MVO 求解。
//!
//! 参考：Black & Litterman (1992) "Global Portfolio Optimization",
//! Financial Analysts Journal.

use super::cov::Matrix;

// ─── 内部工具 ─────────────────────────────────────────────────────

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Cholesky 分解：L L^T = A（正定矩阵）
fn cholesky(a: &Matrix) -> Option<Matrix> {
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

fn cholesky_solve(l: &Matrix, b: &[f64]) -> Vec<f64> {
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

/// 对称矩阵向量乘积：A v
fn mat_vec(a: &Matrix, v: &[f64]) -> Vec<f64> {
    (0..a.rows).map(|r| dot(&a.columns()[r], v)).collect()
}

// ─── Black-Litterman 核心 ──────────────────────────────────────────

/// Black-Litterman 输入参数
pub struct BLInput {
    /// 市场资本化权重（n 维）
    pub market_weights: Vec<f64>,
    /// 年化协方差矩阵（n×n）
    pub cov: Matrix,
    /// 风险厌恶系数 δ（通常取 2.5）
    pub risk_aversion: f64,
    /// 不确定性缩放因子 τ（通常取 0.05 ~ 0.1）
    pub tau: f64,
    /// 观点矩阵 P（k×n），每行表示一个观点
    pub p: Matrix,
    /// 观点收益向量 Q（k 维）
    pub q: Vec<f64>,
    /// 观点不确定性矩阵 Ω（k×k，通常为对角矩阵）
    pub omega: Matrix,
}

/// Black-Litterman 输出
#[derive(Debug, Clone)]
pub struct BLOutput {
    /// 先验（均衡）期望收益率 Π = δ Σ w_mkt
    pub prior_returns: Vec<f64>,
    /// 后验期望收益率
    pub posterior_returns: Vec<f64>,
    /// 后验协方差矩阵
    pub posterior_cov: Matrix,
}

impl BLOutput {
    /// 后验标准差向量
    pub fn posterior_std(&self) -> Vec<f64> {
        (0..self.posterior_cov.rows)
            .map(|i| self.posterior_cov.get(i, i).sqrt())
            .collect()
    }
}

/// 计算 Black-Litterman 后验收益率和协方差
///
/// # 公式
/// Π = δ Σ w_mkt  （均衡期望）
/// M = (τΣ)^{-1} + P^T Ω^{-1} P
/// μ_BL = M^{-1} [ (τΣ)^{-1} Π + P^T Ω^{-1} Q ]
/// Σ_BL = M^{-1} + Σ
pub fn black_litterman(input: &BLInput) -> Option<BLOutput> {
    let n = input.market_weights.len();
    let k = input.q.len();

    // 1. 先验均衡收益率 Π = δ Σ w
    let sigma_w = mat_vec(&input.cov, &input.market_weights);
    let prior: Vec<f64> = sigma_w.iter().map(|x| input.risk_aversion * x).collect();

    // 2. τΣ
    let tau_sigma = input.cov.scale(input.tau);

    // 3. (τΣ)^{-1}
    let l_ts = cholesky(&tau_sigma)?;

    // (τΣ)^{-1} Π
    let ts_inv_pi = cholesky_solve(&l_ts, &prior);

    // 4. Ω^{-1}
    let l_omega = cholesky(&input.omega)?;

    // P^T Ω^{-1} P  (n×n)
    // Ω^{-1} P (k×n)：对 P 的每一列 j 解 Ω x = P[:,j]，结果存为第 j 列
    let mut omega_inv_p = Matrix::zeros(k, n);
    for j in 0..n {
        let p_col: Vec<f64> = (0..k).map(|i| input.p.get(i, j)).collect();
        let col = cholesky_solve(&l_omega, &p_col);
        for i in 0..k {
            omega_inv_p.set(i, j, col[i]);
        }
    }

    // P^T (Ω^{-1} P)  →  n×n
    let mut pt_omega_inv_p = Matrix::zeros(n, n);
    for i in 0..n {
        for j in 0..n {
            let mut s = 0.0;
            for ki in 0..k {
                s += input.p.get(ki, i) * omega_inv_p.get(ki, j);
            }
            pt_omega_inv_p.set(i, j, s);
        }
    }

    // P^T Ω^{-1} Q  (n 维)：先求 Ω^{-1} Q（k 维），再左乘 P^T
    let omega_inv_q = cholesky_solve(&l_omega, &input.q);
    let pt_omega_inv_q: Vec<f64> = (0..n)
        .map(|i| (0..k).map(|ki| input.p.get(ki, i) * omega_inv_q[ki]).sum())
        .collect();

    // 5. M = (τΣ)^{-1} + P^T Ω^{-1} P
    //    这里用 (τΣ)^{-1} 的显式矩阵
    let mut ts_inv = Matrix::zeros(n, n);
    for j in 0..n {
        let mut e = vec![0.0; n];
        e[j] = 1.0;
        let col = cholesky_solve(&l_ts, &e);
        for i in 0..n {
            ts_inv.set(i, j, col[i]);
        }
    }
    let m = ts_inv.add(&pt_omega_inv_p);

    // 6. rhs = (τΣ)^{-1} Π + P^T Ω^{-1} Q
    let rhs: Vec<f64> = ts_inv_pi.iter().zip(&pt_omega_inv_q).map(|(a, b)| a + b).collect();

    // 7. μ_BL = M^{-1} rhs
    let l_m = cholesky(&m)?;
    let posterior_returns = cholesky_solve(&l_m, &rhs);

    // 8. Σ_BL = M^{-1} + Σ
    let mut m_inv = Matrix::zeros(n, n);
    for j in 0..n {
        let mut e = vec![0.0; n];
        e[j] = 1.0;
        let col = cholesky_solve(&l_m, &e);
        for i in 0..n {
            m_inv.set(i, j, col[i]);
        }
    }
    let posterior_cov = m_inv.add(&input.cov);

    Some(BLOutput { prior_returns: prior, posterior_returns, posterior_cov })
}

/// 仅利用市场均衡隐含收益（无额外观点）
///
/// 等价于 `black_litterman` 在 k=0 时的退化情况，直接返回 Π。
pub fn implied_returns(market_weights: &[f64], cov: &Matrix, risk_aversion: f64) -> Vec<f64> {
    let sigma_w = mat_vec(cov, market_weights);
    sigma_w.iter().map(|x| risk_aversion * x).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qarisk::riskmodes::cov::Matrix;

    fn two_asset_input() -> BLInput {
        let n = 2;
        let cov = Matrix {
            data: vec![0.04, 0.012, 0.012, 0.09],
            rows: n, cols: n,
        };
        // 1 个观点：资产 0 相对资产 1 超额收益 2%
        let p = Matrix { data: vec![1.0, -1.0], rows: 1, cols: n };
        let omega = Matrix { data: vec![0.001], rows: 1, cols: 1 };
        BLInput {
            market_weights: vec![0.6, 0.4],
            cov,
            risk_aversion: 2.5,
            tau: 0.05,
            p,
            q: vec![0.02],
            omega,
        }
    }

    #[test]
    fn test_bl_posterior_not_equal_prior() {
        let input = two_asset_input();
        let out = black_litterman(&input).unwrap();
        // 后验与先验不同（观点已产生影响）
        let diff: f64 = out.posterior_returns.iter()
            .zip(&out.prior_returns)
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(diff > 1e-6, "posterior should differ from prior");
    }

    #[test]
    fn test_implied_returns_positive() {
        let cov = Matrix {
            data: vec![0.04, 0.012, 0.012, 0.09],
            rows: 2, cols: 2,
        };
        let ir = implied_returns(&[0.6, 0.4], &cov, 2.5);
        assert_eq!(ir.len(), 2);
        assert!(ir[0] > 0.0 && ir[1] > 0.0);
    }
}
