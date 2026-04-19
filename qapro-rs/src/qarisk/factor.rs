//! 因子风险引擎（Phase 3）
//!
//! 提供：
//! - OLS 因子回归（多因子 CAPM）
//! - Beta 敞口计算
//! - 因子风险分解（系统性风险 vs 特质性风险）
//! - 因子中性化（从组合权重中剔除特定因子暴露）

use crate::qarisk::riskmodes::cov::Matrix;

// ─── OLS 回归结果 ─────────────────────────────────────────────────────────────

/// OLS 回归结果
#[derive(Debug, Clone)]
pub struct OLSResult {
    /// 截距（alpha）
    pub alpha: f64,
    /// 因子载荷（beta 向量，长度 = 因子数）
    pub betas: Vec<f64>,
    /// 残差序列
    pub residuals: Vec<f64>,
    /// 决定系数 R²
    pub r_squared: f64,
    /// 残差标准差（特质性风险）
    pub residual_std: f64,
}

// ─── 因子风险引擎 ─────────────────────────────────────────────────────────────

pub struct FactorRiskEngine {
    /// 因子名称列表
    pub factor_names: Vec<String>,
}

impl FactorRiskEngine {
    pub fn new(factor_names: Vec<String>) -> Self {
        Self { factor_names }
    }

    /// OLS 因子回归
    ///
    /// `asset_returns`：资产日收益率序列（T 维）
    /// `factor_returns`：因子收益率矩阵（T×K），每列一个因子
    ///
    /// 模型：r_t = α + F_t β + ε_t
    pub fn ols_regression(
        &self,
        asset_returns: &[f64],
        factor_returns: &Matrix,
    ) -> Option<OLSResult> {
        let t = asset_returns.len();
        if t < 2 || t != factor_returns.rows {
            return None;
        }
        let k = factor_returns.cols;

        // 构造设计矩阵 X（T×(K+1)，第一列为截距列 1）
        let mut x_data = vec![0.0; t * (k + 1)];
        for i in 0..t {
            x_data[i * (k + 1)] = 1.0; // 截距
            for j in 0..k {
                x_data[i * (k + 1) + j + 1] = factor_returns.get(i, j);
            }
        }
        let x = Matrix { data: x_data, rows: t, cols: k + 1 };

        // β = (X^T X)^{-1} X^T y
        let xt = x.transpose();
        let xtx = xt.matmul(&x);
        let xty: Vec<f64> = (0..k + 1)
            .map(|j| (0..t).map(|i| xt.get(j, i) * asset_returns[i]).sum())
            .collect();

        let coeffs = solve_linear(&xtx, &xty)?;
        let alpha = coeffs[0];
        let betas = coeffs[1..].to_vec();

        // 计算残差
        let y_hat: Vec<f64> = (0..t)
            .map(|i| {
                alpha + (0..k).map(|j| betas[j] * factor_returns.get(i, j)).sum::<f64>()
            })
            .collect();
        let residuals: Vec<f64> = asset_returns.iter().zip(&y_hat).map(|(r, h)| r - h).collect();

        // R²
        let y_mean: f64 = asset_returns.iter().sum::<f64>() / t as f64;
        let ss_tot: f64 = asset_returns.iter().map(|r| (r - y_mean).powi(2)).sum();
        let ss_res: f64 = residuals.iter().map(|e| e.powi(2)).sum();
        let r_squared = if ss_tot < 1e-14 { 0.0 } else { 1.0 - ss_res / ss_tot };

        // 残差标准差
        let residual_std = if t > k + 1 {
            (ss_res / (t - k - 1) as f64).sqrt()
        } else {
            0.0
        };

        Some(OLSResult { alpha, betas, residuals, r_squared, residual_std })
    }

    /// 计算投资组合的因子暴露（加权 Beta）
    ///
    /// `weights`：组合权重向量（N 维）
    /// `asset_betas`：每个资产对每个因子的 Beta 矩阵（N×K）
    ///
    /// 返回：组合因子暴露向量（K 维）
    pub fn portfolio_factor_exposure(
        &self,
        weights: &[f64],
        asset_betas: &Matrix,
    ) -> Vec<f64> {
        let n = weights.len();
        let k = asset_betas.cols;
        assert_eq!(n, asset_betas.rows);
        (0..k)
            .map(|j| (0..n).map(|i| weights[i] * asset_betas.get(i, j)).sum())
            .collect()
    }

    /// 风险分解：系统性方差 vs 特质性方差
    ///
    /// - 系统性方差 = β^T Σ_F β（β 为组合因子暴露，Σ_F 为因子协方差矩阵）
    /// - 特质性方差 = Σ(w_i² σ²_ε_i)（各资产残差方差的加权和）
    /// - 总方差 = 系统性 + 特质性
    pub fn variance_decomposition(
        &self,
        weights: &[f64],
        asset_betas: &Matrix,
        factor_cov: &Matrix,
        residual_vols: &[f64],
    ) -> VarianceDecomposition {
        let portfolio_exposure = self.portfolio_factor_exposure(weights, asset_betas);
        let k = factor_cov.rows;

        // 系统性方差 = β^T Σ_F β
        let mut systematic_var = 0.0;
        for i in 0..k {
            for j in 0..k {
                systematic_var += portfolio_exposure[i] * factor_cov.get(i, j) * portfolio_exposure[j];
            }
        }

        // 特质性方差 = Σ(w_i² σ²_ε_i)
        let idiosyncratic_var: f64 = weights.iter().zip(residual_vols)
            .map(|(w, s)| w * w * s * s)
            .sum();

        VarianceDecomposition {
            systematic_variance: systematic_var,
            idiosyncratic_variance: idiosyncratic_var,
            total_variance: systematic_var + idiosyncratic_var,
        }
    }

    /// 因子中性化：调整权重以消除特定因子的暴露
    ///
    /// 从权重中减去在因子方向上的分量，再归一化。
    pub fn neutralize(
        &self,
        weights: &[f64],
        asset_betas: &Matrix,
        factor_index: usize,
    ) -> Vec<f64> {
        let n = weights.len();
        assert!(factor_index < asset_betas.cols);

        // 计算当前在 factor_index 因子上的组合暴露
        let portfolio_beta: f64 = (0..n)
            .map(|i| weights[i] * asset_betas.get(i, factor_index))
            .sum();

        // 因子载荷向量（N 维）
        let factor_loadings: Vec<f64> = (0..n)
            .map(|i| asset_betas.get(i, factor_index))
            .collect();

        // 因子方差 ||β||²
        let factor_var: f64 = factor_loadings.iter().map(|b| b * b).sum();
        if factor_var < 1e-14 {
            return weights.to_vec();
        }

        // 中性化：w_neutral = w - (portfolio_beta / factor_var) × factor_loadings
        let scale = portfolio_beta / factor_var;
        let mut neutral_w: Vec<f64> = weights.iter().zip(&factor_loadings)
            .map(|(w, b)| w - scale * b)
            .collect();

        // 归一化（可能需要处理负权重，这里简单截断到 0）
        for wi in &mut neutral_w { *wi = wi.max(0.0); }
        let sum: f64 = neutral_w.iter().sum();
        if sum > 1e-14 {
            for wi in &mut neutral_w { *wi /= sum; }
        }
        neutral_w
    }
}

/// 风险分解结果
#[derive(Debug, Clone)]
pub struct VarianceDecomposition {
    /// 系统性方差（因子驱动）
    pub systematic_variance: f64,
    /// 特质性方差（个股 Alpha）
    pub idiosyncratic_variance: f64,
    /// 总方差
    pub total_variance: f64,
}

impl VarianceDecomposition {
    /// 系统性风险占比
    pub fn systematic_pct(&self) -> f64 {
        if self.total_variance < 1e-14 { 0.0 } else { self.systematic_variance / self.total_variance }
    }
}

// ─── 内部工具：高斯消元求线性方程组 ──────────────────────────────────────────

fn solve_linear(a: &Matrix, b: &[f64]) -> Option<Vec<f64>> {
    let n = a.rows;
    assert_eq!(n, a.cols);
    assert_eq!(n, b.len());

    // 构造增广矩阵
    let mut aug: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            let mut row: Vec<f64> = (0..n).map(|j| a.get(i, j)).collect();
            row.push(b[i]);
            row
        })
        .collect();

    // 高斯消元（部分主元）
    for col in 0..n {
        // 找主元
        let pivot_row = (col..n)
            .max_by(|&i, &j| aug[i][col].abs().partial_cmp(&aug[j][col].abs()).unwrap())?;
        aug.swap(col, pivot_row);

        let pivot = aug[col][col];
        if pivot.abs() < 1e-12 { return None; }

        for row in (col + 1)..n {
            let factor = aug[row][col] / pivot;
            for c in col..=n {
                let v = aug[col][c];
                aug[row][c] -= factor * v;
            }
        }
    }

    // 回代
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = aug[i][n];
        for j in (i + 1)..n { s -= aug[i][j] * x[j]; }
        x[i] = s / aug[i][i];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qarisk::riskmodes::cov::Matrix;

    fn make_factor_data() -> (Vec<f64>, Matrix) {
        // 10 个交易日，1 个因子（市场收益率）
        let factor_returns = vec![
            0.01, -0.02, 0.015, -0.005, 0.02,
            -0.01, 0.008, -0.015, 0.012, -0.003,
        ];
        // 资产收益 ≈ 0.02 + 1.5 × factor + noise
        let asset_returns: Vec<f64> = factor_returns.iter()
            .enumerate()
            .map(|(i, f)| 0.02 / 252.0 + 1.5 * f + (i as f64 * 0.001 - 0.004))
            .collect();
        let factor_matrix = Matrix {
            data: factor_returns,
            rows: 10,
            cols: 1,
        };
        (asset_returns, factor_matrix)
    }

    #[test]
    fn test_ols_regression_r_squared() {
        let engine = FactorRiskEngine::new(vec!["market".into()]);
        let (asset_ret, factor_matrix) = make_factor_data();
        let res = engine.ols_regression(&asset_ret, &factor_matrix).unwrap();
        // Beta 应接近 1.5
        assert!((res.betas[0] - 1.5).abs() < 0.1, "beta={}", res.betas[0]);
        assert!(res.r_squared > 0.90, "R²={}", res.r_squared);
    }

    #[test]
    fn test_portfolio_factor_exposure() {
        let engine = FactorRiskEngine::new(vec!["mkt".into()]);
        let weights = vec![0.6, 0.4];
        let asset_betas = Matrix {
            data: vec![1.2, 0.8], // asset 0 beta=1.2, asset 1 beta=0.8
            rows: 2, cols: 1,
        };
        let exposure = engine.portfolio_factor_exposure(&weights, &asset_betas);
        let expected = 0.6 * 1.2 + 0.4 * 0.8; // = 1.04
        assert!((exposure[0] - expected).abs() < 1e-10);
    }

    #[test]
    fn test_variance_decomposition() {
        let engine = FactorRiskEngine::new(vec!["mkt".into()]);
        let weights = vec![0.5, 0.5];
        let asset_betas = Matrix { data: vec![1.0, 0.8], rows: 2, cols: 1 };
        let factor_cov = Matrix { data: vec![0.04], rows: 1, cols: 1 }; // 市场波动 20%
        let residual_vols = vec![0.1, 0.12]; // 特质性 10%/12%
        let vd = engine.variance_decomposition(&weights, &asset_betas, &factor_cov, &residual_vols);
        assert!(vd.total_variance > 0.0);
        assert!(vd.systematic_pct() > 0.0 && vd.systematic_pct() < 1.0);
    }

    #[test]
    fn test_neutralize_reduces_exposure() {
        let engine = FactorRiskEngine::new(vec!["size".into()]);
        let weights = vec![0.5, 0.5];
        let asset_betas = Matrix { data: vec![2.0, -1.0], rows: 2, cols: 1 };
        let neutral = engine.neutralize(&weights, &asset_betas, 0);
        // 中性化后因子暴露应接近 0
        let exposure: f64 = neutral.iter().zip([2.0f64, -1.0].iter())
            .map(|(w, b)| w * b).sum();
        assert!(exposure.abs() < 0.1, "exposure after neutralization: {}", exposure);
    }
}
