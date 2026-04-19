#![allow(dead_code)]
//! 协方差矩阵估计
//!
//! 提供用于投资组合风险建模的协方差矩阵计算，包括：
//! - 样本协方差矩阵
//! - 年化协方差矩阵
//! - 相关系数矩阵

/// 简单二维矩阵（行优先存储）
#[derive(Debug, Clone)]
pub struct Matrix {
    pub data: Vec<f64>,
    pub rows: usize,
    pub cols: usize,
}

impl Matrix {
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self { data: vec![0.0; rows * cols], rows, cols }
    }

    pub fn identity(n: usize) -> Self {
        let mut m = Self::zeros(n, n);
        for i in 0..n {
            m[(i, i)] = 1.0;
        }
        m
    }

    pub fn get(&self, r: usize, c: usize) -> f64 {
        self.data[r * self.cols + c]
    }

    pub fn set(&mut self, r: usize, c: usize, v: f64) {
        self.data[r * self.cols + c] = v;
    }

    pub fn transpose(&self) -> Self {
        let mut out = Self::zeros(self.cols, self.rows);
        for r in 0..self.rows {
            for c in 0..self.cols {
                out.set(c, r, self.get(r, c));
            }
        }
        out
    }

    /// 矩阵乘法
    pub fn matmul(&self, other: &Matrix) -> Matrix {
        assert_eq!(self.cols, other.rows, "matmul dimension mismatch");
        let mut out = Matrix::zeros(self.rows, other.cols);
        for i in 0..self.rows {
            for k in 0..self.cols {
                let a = self.get(i, k);
                if a == 0.0 { continue; }
                for j in 0..other.cols {
                    let v = out.get(i, j) + a * other.get(k, j);
                    out.set(i, j, v);
                }
            }
        }
        out
    }

    pub fn scale(&self, s: f64) -> Self {
        Self { data: self.data.iter().map(|x| x * s).collect(), rows: self.rows, cols: self.cols }
    }

    pub fn add(&self, other: &Matrix) -> Matrix {
        assert_eq!(self.rows, other.rows);
        assert_eq!(self.cols, other.cols);
        Self {
            data: self.data.iter().zip(&other.data).map(|(a, b)| a + b).collect(),
            rows: self.rows,
            cols: self.cols,
        }
    }

    /// 对角元素向量
    pub fn diagonal(&self) -> Vec<f64> {
        (0..self.rows.min(self.cols)).map(|i| self.get(i, i)).collect()
    }

    /// Frobenius 范数
    pub fn frobenius_norm(&self) -> f64 {
        self.data.iter().map(|x| x * x).sum::<f64>().sqrt()
    }

    /// 逐列提取为 Vec<Vec<f64>>（每个内层 Vec 是一列）
    pub fn columns(&self) -> Vec<Vec<f64>> {
        (0..self.cols)
            .map(|c| (0..self.rows).map(|r| self.get(r, c)).collect())
            .collect()
    }
}

impl std::ops::Index<(usize, usize)> for Matrix {
    type Output = f64;
    fn index(&self, (r, c): (usize, usize)) -> &f64 {
        &self.data[r * self.cols + c]
    }
}

impl std::ops::IndexMut<(usize, usize)> for Matrix {
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut f64 {
        &mut self.data[r * self.cols + c]
    }
}

// ─── 统计辅助 ─────────────────────────────────────────────────────

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn variance(v: &[f64]) -> f64 {
    let m = mean(v);
    let n = (v.len() - 1) as f64;
    v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n
}

fn std_dev(v: &[f64]) -> f64 {
    variance(v).sqrt()
}

fn covariance(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    let ma = mean(a);
    let mb = mean(b);
    let n = (a.len() - 1) as f64;
    a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum::<f64>() / n
}

// ─── 公开 API ─────────────────────────────────────────────────────

/// 从收益率矩阵计算样本协方差矩阵
///
/// `returns`：T×N 矩阵，T 为时间步，N 为资产数
pub fn sample_cov(returns: &Matrix) -> Matrix {
    let n = returns.cols;
    let cols = returns.columns();
    let mut cov = Matrix::zeros(n, n);
    for i in 0..n {
        for j in i..n {
            let c = covariance(&cols[i], &cols[j]);
            cov.set(i, j, c);
            cov.set(j, i, c);
        }
    }
    cov
}

/// 计算各资产年化均值收益率（假设 `trading_days` 个交易日/年）
pub fn annualized_returns(returns: &Matrix, trading_days: f64) -> Vec<f64> {
    returns.columns().iter().map(|col| mean(col) * trading_days).collect()
}

/// 计算年化协方差矩阵
pub fn annualized_cov(returns: &Matrix, trading_days: f64) -> Matrix {
    sample_cov(returns).scale(trading_days)
}

/// 从协方差矩阵计算相关系数矩阵
pub fn correlation(cov: &Matrix) -> Matrix {
    let n = cov.rows;
    let std_devs: Vec<f64> = (0..n).map(|i| cov.get(i, i).sqrt()).collect();
    let mut corr = Matrix::zeros(n, n);
    for i in 0..n {
        for j in 0..n {
            if std_devs[i] > 0.0 && std_devs[j] > 0.0 {
                corr.set(i, j, cov.get(i, j) / (std_devs[i] * std_devs[j]));
            }
        }
    }
    corr
}

/// 计算投资组合方差：w^T Σ w
pub fn portfolio_variance(weights: &[f64], cov: &Matrix) -> f64 {
    let n = weights.len();
    assert_eq!(n, cov.rows);
    let mut var = 0.0;
    for i in 0..n {
        for j in 0..n {
            var += weights[i] * weights[j] * cov.get(i, j);
        }
    }
    var
}

/// 计算投资组合标准差
pub fn portfolio_std(weights: &[f64], cov: &Matrix) -> f64 {
    portfolio_variance(weights, cov).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_returns() -> Matrix {
        // 5 个交易日，2 只资产
        let data = vec![
            0.01, 0.02,
            -0.01, 0.01,
            0.02, -0.01,
            0.00, 0.03,
            0.01, 0.00,
        ];
        Matrix { data, rows: 5, cols: 2 }
    }

    #[test]
    fn test_sample_cov_symmetry() {
        let r = make_returns();
        let cov = sample_cov(&r);
        assert!((cov.get(0, 1) - cov.get(1, 0)).abs() < 1e-12);
    }

    #[test]
    fn test_correlation_diagonal_ones() {
        let r = make_returns();
        let cov = sample_cov(&r);
        let corr = correlation(&cov);
        assert!((corr.get(0, 0) - 1.0).abs() < 1e-12);
        assert!((corr.get(1, 1) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_portfolio_variance_equal_weight() {
        let r = make_returns();
        let cov = sample_cov(&r);
        let w = vec![0.5, 0.5];
        let pv = portfolio_variance(&w, &cov);
        assert!(pv >= 0.0);
    }

    #[test]
    fn test_annualized_cov_scale() {
        let r = make_returns();
        let cov = sample_cov(&r);
        let acov = annualized_cov(&r, 252.0);
        assert!((acov.get(0, 0) - cov.get(0, 0) * 252.0).abs() < 1e-12);
    }
}
