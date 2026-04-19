//! PyO3 Python 绑定（feature = "pyo3-bindings"）
//!
//! 暴露给 Python 的模块名为 `qapro_rs`，API 与 qars2 保持兼容：
//!
//! ```python
//! from qapro_rs import ma, ema, macd, rsi, boll, atr, QADataFrame, PyQAAccount
//!
//! # 向量化指标函数
//! ma5  = ma(close_list, 5)
//! ema20 = ema(close_list, 20)
//! result = macd(close_list)            # dict: macd/signal/histogram
//! upper, mid, lower = boll(close_list) # dict: upper/mid/lower
//!
//! # QADataFrame —— 贴近 qars2.QADataFrame
//! df = QADataFrame({'open': [...], 'high': [...], 'low': [...], 'close': [...], 'volume': [...]})
//! df.ma(5)
//! df.macd()
//!
//! # 并行计算
//! results = parallel_apply(list_of_close_arrays, 'ma', period=5)
//! ```
//!
//! 构建方式：
//! ```bash
//! pip install maturin
//! maturin develop --features pyo3-bindings
//! ```

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::qaaccount::account::QA_Account;
use crate::qaindicator::{Next, Reset};
use crate::qaindicator::indicators::{
    SimpleMovingAverage as Sma,
    ExponentialMovingAverage as Ema,
    MovingAverageConvergenceDivergence as Macd,
    RelativeStrengthIndex as Rsi,
    BollingerBands as Boll,
    AverageTrueRange as Atr,
    RateOfChange as Roc,
    StandardDeviation as Std,
    HHV,
    LLV,
    FastStochastic,
    SlowStochastic,
    OnBalanceVolume,
};

// ─── 辅助：OHLCV 数据点（用于需要 High/Low/Close/Volume 的指标）─────────────

struct Bar {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

impl crate::qaindicator::Close for Bar {
    fn close(&self) -> f64 { self.close }
}
impl crate::qaindicator::High for Bar {
    fn high(&self) -> f64 { self.high }
}
impl crate::qaindicator::Low for Bar {
    fn low(&self) -> f64 { self.low }
}
impl crate::qaindicator::Open for Bar {
    fn open(&self) -> f64 { self.open }
}
impl crate::qaindicator::Volume for Bar {
    fn volume(&self) -> f64 { self.volume }
}

// ─── 填充 NaN 的长度前缀（预热期输出 f64::NAN）────────────────────────────────

fn pad(out: Vec<f64>, total: usize) -> Vec<f64> {
    let pad_len = total.saturating_sub(out.len());
    let mut result = vec![f64::NAN; pad_len];
    result.extend(out);
    result
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 向量化指标函数
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 简单移动平均 MA(n)
///
/// Args:
///     values: 价格序列（Python list 或可迭代对象）
///     period: 周期，默认 5
///
/// Returns:
///     list[float]，长度与输入相同，预热期为 NaN
#[pyfunction]
#[pyo3(signature = (values, period=5))]
pub fn ma(values: Vec<f64>, period: u32) -> Vec<f64> {
    let n = values.len();
    let mut ind = match Sma::new(period) {
        Ok(v) => v,
        Err(_) => return vec![f64::NAN; n],
    };
    let out: Vec<f64> = values.iter().map(|&v| ind.next(v)).collect();
    pad(out, n)
}

/// 指数移动平均 EMA(n)
#[pyfunction]
#[pyo3(signature = (values, period=12))]
pub fn ema(values: Vec<f64>, period: u32) -> Vec<f64> {
    let n = values.len();
    let mut ind = match Ema::new(period) {
        Ok(v) => v,
        Err(_) => return vec![f64::NAN; n],
    };
    let out: Vec<f64> = values.iter().map(|&v| ind.next(v)).collect();
    pad(out, n)
}

/// MACD(fast, slow, signal)
///
/// Returns:
///     dict with keys 'macd', 'signal', 'histogram', each a list[float]
#[pyfunction]
#[pyo3(signature = (values, fast=12, slow=26, signal=9))]
pub fn macd(py: Python<'_>, values: Vec<f64>, fast: u32, slow: u32, signal: u32) -> PyObject {
    let n = values.len();
    let mut ind = match Macd::new(fast, slow, signal) {
        Ok(v) => v,
        Err(_) => {
            let d = PyDict::new(py);
            let nan = vec![f64::NAN; n];
            d.set_item("macd", nan.clone()).ok();
            d.set_item("signal", nan.clone()).ok();
            d.set_item("histogram", nan).ok();
            return d.into();
        }
    };
    let mut macd_v = Vec::with_capacity(n);
    let mut signal_v = Vec::with_capacity(n);
    let mut hist_v = Vec::with_capacity(n);
    for &v in &values {
        let (m, s, h) = ind.next(v);
        macd_v.push(m);
        signal_v.push(s);
        hist_v.push(h);
    }
    let d = PyDict::new(py);
    d.set_item("macd",      macd_v).ok();
    d.set_item("signal",    signal_v).ok();
    d.set_item("histogram", hist_v).ok();
    d.into()
}

/// RSI(n)
#[pyfunction]
#[pyo3(signature = (values, period=14))]
pub fn rsi(values: Vec<f64>, period: u32) -> Vec<f64> {
    let n = values.len();
    let mut ind = match Rsi::new(period) {
        Ok(v) => v,
        Err(_) => return vec![f64::NAN; n],
    };
    let out: Vec<f64> = values.iter().map(|&v| ind.next(v)).collect();
    pad(out, n)
}

/// 布林带 BOLL(period, std_dev)
///
/// Returns:
///     dict with keys 'upper', 'mid', 'lower'
#[pyfunction]
#[pyo3(signature = (values, period=20, std_dev=2.0))]
pub fn boll(py: Python<'_>, values: Vec<f64>, period: u32, std_dev: f64) -> PyObject {
    let n = values.len();
    let mut ind = match Boll::new(period, std_dev) {
        Ok(v) => v,
        Err(_) => {
            let d = PyDict::new(py);
            let nan = vec![f64::NAN; n];
            d.set_item("upper", nan.clone()).ok();
            d.set_item("mid",   nan.clone()).ok();
            d.set_item("lower", nan).ok();
            return d.into();
        }
    };
    let mut upper = Vec::with_capacity(n);
    let mut mid   = Vec::with_capacity(n);
    let mut lower = Vec::with_capacity(n);
    for &v in &values {
        let out = ind.next(v);
        upper.push(out.upper);
        mid.push(out.average);
        lower.push(out.lower);
    }
    let d = PyDict::new(py);
    d.set_item("upper", upper).ok();
    d.set_item("mid",   mid).ok();
    d.set_item("lower", lower).ok();
    d.into()
}

/// 平均真实波幅 ATR(period)
///
/// Args:
///     high, low, close: 价格序列（等长）
///     period: 默认 14
#[pyfunction]
#[pyo3(signature = (high, low, close, period=14))]
pub fn atr(high: Vec<f64>, low: Vec<f64>, close: Vec<f64>, period: u32) -> Vec<f64> {
    let n = close.len();
    if high.len() != n || low.len() != n {
        return vec![f64::NAN; n];
    }
    let mut ind = match Atr::new(period) {
        Ok(v) => v,
        Err(_) => return vec![f64::NAN; n],
    };
    let out: Vec<f64> = (0..n)
        .map(|i| {
            let bar = Bar { open: close[i], high: high[i], low: low[i], close: close[i], volume: 0.0 };
            ind.next(&bar)
        })
        .collect();
    pad(out, n)
}

/// KDJ 随机指标 —— K/D/J 三线
///
/// J = 3K - 2D
///
/// Returns:
///     dict with keys 'k', 'd', 'j'
#[pyfunction]
#[pyo3(signature = (high, low, close, n=9, m1=3, m2=3))]
pub fn kdj(py: Python<'_>, high: Vec<f64>, low: Vec<f64>, close: Vec<f64>,
           n: u32, m1: u32, m2: u32) -> PyObject {
    let total = close.len();
    if high.len() != total || low.len() != total {
        let d = PyDict::new(py);
        let nan = vec![f64::NAN; total];
        d.set_item("k", nan.clone()).ok();
        d.set_item("d", nan.clone()).ok();
        d.set_item("j", nan).ok();
        return d.into();
    }
    let mut fast = match FastStochastic::new(n) {
        Ok(v) => v,
        Err(_) => {
            let d = PyDict::new(py);
            let nan = vec![f64::NAN; total];
            d.set_item("k", nan.clone()).ok();
            d.set_item("d", nan.clone()).ok();
            d.set_item("j", nan).ok();
            return d.into();
        }
    };
    let mut slow = match SlowStochastic::new(n, m1) {
        Ok(v) => v,
        Err(_) => {
            let d = PyDict::new(py);
            let nan = vec![f64::NAN; total];
            d.set_item("k", nan.clone()).ok();
            d.set_item("d", nan.clone()).ok();
            d.set_item("j", nan).ok();
            return d.into();
        }
    };
    let _ = m2; // D 的平滑在 SlowStochastic 内部处理

    let mut k_vec = Vec::with_capacity(total);
    let mut d_vec = Vec::with_capacity(total);
    let mut j_vec = Vec::with_capacity(total);

    for i in 0..total {
        let bar = Bar { open: close[i], high: high[i], low: low[i], close: close[i], volume: 0.0 };
        let k = fast.next(&bar);
        let d = slow.next(&bar);
        let j = 3.0 * k - 2.0 * d;
        k_vec.push(k);
        d_vec.push(d);
        j_vec.push(j);
    }

    let d = PyDict::new(py);
    d.set_item("k", k_vec).ok();
    d.set_item("d", d_vec).ok();
    d.set_item("j", j_vec).ok();
    d.into()
}

/// 能量潮 OBV
///
/// Args:
///     close, volume: 等长序列
#[pyfunction]
pub fn obv(close: Vec<f64>, volume: Vec<f64>) -> Vec<f64> {
    let n = close.len();
    if volume.len() != n {
        return vec![f64::NAN; n];
    }
    let mut ind = OnBalanceVolume::new();
    (0..n)
        .map(|i| {
            let bar = Bar { open: close[i], high: close[i], low: close[i], close: close[i], volume: volume[i] };
            ind.next(&bar)
        })
        .collect()
}

/// 最高值 HHV(period)
#[pyfunction]
#[pyo3(signature = (values, period=5))]
pub fn hhv(values: Vec<f64>, period: u32) -> Vec<f64> {
    let n = values.len();
    let mut ind = match HHV::new(period) {
        Ok(v) => v,
        Err(_) => return vec![f64::NAN; n],
    };
    values.iter().map(|&v| ind.next(v)).collect()
}

/// 最低值 LLV(period)
#[pyfunction]
#[pyo3(signature = (values, period=5))]
pub fn llv(values: Vec<f64>, period: u32) -> Vec<f64> {
    let n = values.len();
    let mut ind = match LLV::new(period) {
        Ok(v) => v,
        Err(_) => return vec![f64::NAN; n],
    };
    values.iter().map(|&v| ind.next(v)).collect()
}

/// 变动率 ROC(period) —— (close - close[n]) / close[n] * 100
#[pyfunction]
#[pyo3(signature = (values, period=12))]
pub fn roc(values: Vec<f64>, period: u32) -> Vec<f64> {
    let n = values.len();
    let mut ind = match Roc::new(period) {
        Ok(v) => v,
        Err(_) => return vec![f64::NAN; n],
    };
    let out: Vec<f64> = values.iter().map(|&v| ind.next(v)).collect();
    pad(out, n)
}

/// 价格动量 momentum(period) —— close / close[n] - 1，与 ROC 等价（返回小数）
#[pyfunction]
#[pyo3(signature = (values, period=12))]
pub fn momentum(values: Vec<f64>, period: u32) -> Vec<f64> {
    let n = values.len();
    let mut ind = match Roc::new(period) {
        Ok(v) => v,
        Err(_) => return vec![f64::NAN; n],
    };
    let out: Vec<f64> = values.iter().map(|&v| ind.next(v) / 100.0).collect();
    pad(out, n)
}

/// 简单收益率 returns() —— (close[t] - close[t-1]) / close[t-1]
#[pyfunction]
pub fn returns(values: Vec<f64>) -> Vec<f64> {
    let n = values.len();
    if n == 0 { return vec![]; }
    let mut out = vec![f64::NAN];
    for i in 1..n {
        let prev = values[i - 1];
        out.push(if prev != 0.0 { (values[i] - prev) / prev } else { f64::NAN });
    }
    out
}

/// 滚动波动率（滚动标准差）volatility(period)
#[pyfunction]
#[pyo3(signature = (values, period=20))]
pub fn volatility(values: Vec<f64>, period: u32) -> Vec<f64> {
    let n = values.len();
    let mut ind = match Std::new(period) {
        Ok(v) => v,
        Err(_) => return vec![f64::NAN; n],
    };
    let out: Vec<f64> = values.iter().map(|&v| ind.next(v)).collect();
    pad(out, n)
}

/// 滚动相关系数 correlation(x, y, period)
#[pyfunction]
#[pyo3(signature = (x, y, period=20))]
pub fn correlation(x: Vec<f64>, y: Vec<f64>, period: u32) -> Vec<f64> {
    let n = x.len();
    if y.len() != n { return vec![f64::NAN; n]; }
    let p = period as usize;
    (0..n).map(|i| {
        if i + 1 < p { return f64::NAN; }
        let start = i + 1 - p;
        let xs = &x[start..=i];
        let ys = &y[start..=i];
        let mx: f64 = xs.iter().sum::<f64>() / p as f64;
        let my: f64 = ys.iter().sum::<f64>() / p as f64;
        let cov: f64 = xs.iter().zip(ys.iter()).map(|(&a, &b)| (a - mx) * (b - my)).sum::<f64>() / p as f64;
        let sx: f64 = (xs.iter().map(|&a| (a - mx).powi(2)).sum::<f64>() / p as f64).sqrt();
        let sy: f64 = (ys.iter().map(|&b| (b - my).powi(2)).sum::<f64>() / p as f64).sqrt();
        if sx == 0.0 || sy == 0.0 { f64::NAN } else { cov / (sx * sy) }
    }).collect()
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// QADataFrame —— 贴近 qars2.QADataFrame
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Python 高性能数据帧，兼容 qars2.QADataFrame
///
/// 接受 pandas DataFrame 或 dict（含 open/high/low/close/volume 列），提供
/// 指标计算、Arrow 转换和 pandas 输出。
///
/// 示例:
/// ```python
/// import pandas as pd
/// from qapro_rs import QADataFrame
///
/// df = QADataFrame(stock_df)          # pandas DataFrame 或 dict
/// ma5  = df.ma(5)                     # list[float]
/// macd = df.macd()                    # dict
/// result = df.to_dict()               # 含所有原始列的 dict
/// ```
#[pyclass(name = "QADataFrame")]
pub struct PyQADataFrame {
    data: HashMap<String, Vec<f64>>,
}

fn extract_col(data: &HashMap<String, Vec<f64>>, col: &str) -> Option<Vec<f64>> {
    data.get(col).cloned()
}

fn dict_to_hashmap(py: Python<'_>, obj: &PyAny) -> PyResult<HashMap<String, Vec<f64>>> {
    let mut map = HashMap::new();
    // 支持 pandas DataFrame（调用 .to_dict('list')）和普通 dict
    let d: &PyDict = if obj.hasattr("to_dict")? {
        let dict_obj = obj.call_method1("to_dict", ("list",))?;
        dict_obj.downcast::<PyDict>()?
    } else {
        obj.downcast::<PyDict>()?
    };
    for (k, v) in d.iter() {
        let key: String = k.extract()?;
        // 列可能是 list, numpy array, pandas Series
        let vals: Vec<f64> = if v.hasattr("tolist")? {
            v.call_method0("tolist")?.extract()?
        } else {
            v.extract()?
        };
        map.insert(key, vals);
    }
    Ok(map)
}

#[pymethods]
impl PyQADataFrame {
    /// 构造函数
    ///
    /// Args:
    ///     data: pandas DataFrame 或 dict，需含 'close' 列；高低开量列可选
    #[new]
    pub fn new(py: Python<'_>, data: &PyAny) -> PyResult<Self> {
        let map = dict_to_hashmap(py, data)?;
        Ok(PyQADataFrame { data: map })
    }

    /// 从 PyArrow Table 构造（零拷贝替代方案，先转为 pandas dict）
    #[staticmethod]
    pub fn from_arrow(py: Python<'_>, table: &PyAny) -> PyResult<Self> {
        // 调用 pyarrow table.to_pydict()
        let d = table.call_method0("to_pydict")?;
        let map = dict_to_hashmap(py, d)?;
        Ok(PyQADataFrame { data: map })
    }

    // ── 指标方法 ──────────────────────────────────────────────────────────────

    #[pyo3(signature = (period=5, col="close"))]
    pub fn ma(&self, period: u32, col: &str) -> Vec<f64> {
        match extract_col(&self.data, col) {
            Some(v) => {
                let n = v.len();
                let mut ind = match Sma::new(period) {
                    Ok(x) => x,
                    Err(_) => return vec![f64::NAN; n],
                };
                let out: Vec<f64> = v.iter().map(|&x| ind.next(x)).collect();
                pad(out, n)
            }
            None => vec![],
        }
    }

    #[pyo3(signature = (period=12, col="close"))]
    pub fn ema(&self, period: u32, col: &str) -> Vec<f64> {
        match extract_col(&self.data, col) {
            Some(v) => {
                let n = v.len();
                let mut ind = match Ema::new(period) {
                    Ok(x) => x,
                    Err(_) => return vec![f64::NAN; n],
                };
                let out: Vec<f64> = v.iter().map(|&x| ind.next(x)).collect();
                pad(out, n)
            }
            None => vec![],
        }
    }

    #[pyo3(signature = (fast=12, slow=26, signal=9))]
    pub fn macd(&self, py: Python<'_>, fast: u32, slow: u32, signal: u32) -> PyObject {
        let close = extract_col(&self.data, "close").unwrap_or_default();
        crate::qapyo3::macd(py, close, fast, slow, signal)
    }

    #[pyo3(signature = (period=14))]
    pub fn rsi(&self, period: u32) -> Vec<f64> {
        let close = extract_col(&self.data, "close").unwrap_or_default();
        crate::qapyo3::rsi(close, period)
    }

    #[pyo3(signature = (period=20, std_dev=2.0))]
    pub fn boll(&self, py: Python<'_>, period: u32, std_dev: f64) -> PyObject {
        let close = extract_col(&self.data, "close").unwrap_or_default();
        crate::qapyo3::boll(py, close, period, std_dev)
    }

    #[pyo3(signature = (period=14))]
    pub fn atr(&self, period: u32) -> Vec<f64> {
        let high  = extract_col(&self.data, "high").unwrap_or_default();
        let low   = extract_col(&self.data, "low").unwrap_or_default();
        let close = extract_col(&self.data, "close").unwrap_or_default();
        crate::qapyo3::atr(high, low, close, period)
    }

    #[pyo3(signature = (n=9, m1=3, m2=3))]
    pub fn kdj(&self, py: Python<'_>, n: u32, m1: u32, m2: u32) -> PyObject {
        let high  = extract_col(&self.data, "high").unwrap_or_default();
        let low   = extract_col(&self.data, "low").unwrap_or_default();
        let close = extract_col(&self.data, "close").unwrap_or_default();
        crate::qapyo3::kdj(py, high, low, close, n, m1, m2)
    }

    pub fn obv(&self) -> Vec<f64> {
        let close  = extract_col(&self.data, "close").unwrap_or_default();
        let volume = extract_col(&self.data, "volume").unwrap_or_default();
        crate::qapyo3::obv(close, volume)
    }

    #[pyo3(signature = (period=12))]
    pub fn momentum(&self, period: u32) -> Vec<f64> {
        let close = extract_col(&self.data, "close").unwrap_or_default();
        crate::qapyo3::momentum(close, period)
    }

    pub fn returns(&self) -> Vec<f64> {
        let close = extract_col(&self.data, "close").unwrap_or_default();
        crate::qapyo3::returns(close)
    }

    #[pyo3(signature = (period=20))]
    pub fn volatility(&self, period: u32) -> Vec<f64> {
        let close = extract_col(&self.data, "close").unwrap_or_default();
        crate::qapyo3::volatility(close, period)
    }

    // ── 数据导出 ──────────────────────────────────────────────────────────────

    /// 导出为 Python dict（列名 → list[float]）
    pub fn to_dict(&self, py: Python<'_>) -> PyObject {
        let d = PyDict::new(py);
        for (k, v) in &self.data {
            d.set_item(k, v.clone()).ok();
        }
        d.into()
    }

    /// 转换为 PyArrow Table（需要 pyarrow 已安装）
    pub fn to_arrow(&self, py: Python<'_>) -> PyResult<PyObject> {
        let pa = py.import("pyarrow")?;
        let d = self.to_dict(py);
        let table = pa.call_method1("table", (d,))?;
        Ok(table.into())
    }

    /// 长度（行数）
    pub fn __len__(&self) -> usize {
        self.data.get("close").map(|v| v.len())
            .or_else(|| self.data.values().next().map(|v| v.len()))
            .unwrap_or(0)
    }

    pub fn __repr__(&self) -> String {
        let rows = self.__len__();
        let cols: Vec<&str> = self.data.keys().map(|s| s.as_str()).collect();
        format!("QADataFrame(rows={}, cols={:?})", rows, cols)
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Arrow 顶层函数
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 从 PyArrow Table 创建 QADataFrame（贴近 qars2.from_arrow）
#[pyfunction]
pub fn from_arrow(py: Python<'_>, table: &PyAny) -> PyResult<PyQADataFrame> {
    PyQADataFrame::from_arrow(py, table)
}

/// 处理 PyArrow Table，返回带指标的 dict（贴近 qars2.process_arrow）
///
/// Args:
///     table: PyArrow Table，需含 close 列
///     ops:   要计算的指标列表，如 ['ma5', 'ema20', 'macd', 'rsi14']
///            格式：`<名称><周期>`，未指定周期则用默认值
///
/// Returns:
///     dict，原始列 + 指标列
#[pyfunction]
#[pyo3(signature = (table, ops=None))]
pub fn process_arrow(py: Python<'_>, table: &PyAny, ops: Option<Vec<String>>) -> PyResult<PyObject> {
    let df = PyQADataFrame::from_arrow(py, table)?;
    let result = PyDict::new(py);

    // 复制原始列
    for (k, v) in &df.data {
        result.set_item(k, v.clone())?;
    }

    // 计算请求的指标
    let ops = ops.unwrap_or_else(|| vec![
        "ma5".to_string(), "ma10".to_string(), "ma20".to_string(),
        "ema12".to_string(), "ema26".to_string(),
        "macd".to_string(), "rsi14".to_string(),
    ]);

    for op in &ops {
        let lower = op.to_lowercase();
        // 解析 "ma5" → name="ma", period=5
        let (name, period_str) = lower.trim()
            .split_at(lower.find(|c: char| c.is_ascii_digit()).unwrap_or(lower.len()));
        let period: u32 = period_str.parse().unwrap_or(0);

        match name {
            "ma" => {
                let p = if period == 0 { 5 } else { period };
                let close = extract_col(&df.data, "close").unwrap_or_default();
                result.set_item(format!("ma{}", p), crate::qapyo3::ma(close, p))?;
            }
            "ema" => {
                let p = if period == 0 { 12 } else { period };
                let close = extract_col(&df.data, "close").unwrap_or_default();
                result.set_item(format!("ema{}", p), crate::qapyo3::ema(close, p))?;
            }
            "macd" => {
                let close = extract_col(&df.data, "close").unwrap_or_default();
                let m = crate::qapyo3::macd(py, close, 12, 26, 9);
                result.set_item("macd", m)?;
            }
            "rsi" => {
                let p = if period == 0 { 14 } else { period };
                let close = extract_col(&df.data, "close").unwrap_or_default();
                result.set_item(format!("rsi{}", p), crate::qapyo3::rsi(close, p))?;
            }
            "boll" => {
                let p = if period == 0 { 20 } else { period };
                let close = extract_col(&df.data, "close").unwrap_or_default();
                result.set_item("boll", crate::qapyo3::boll(py, close, p, 2.0))?;
            }
            _ => {}
        }
    }

    Ok(result.into())
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 并行计算
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 对多个价格序列并行应用同一指标（贴近 qars2.parallel_process）
///
/// Args:
///     arrays:     list of list[float]，每个元素是一只股票的价格序列
///     func_name:  指标名称，如 'ma', 'ema', 'rsi', 'roc', 'volatility'
///     period:     指标周期，默认 5
///
/// Returns:
///     list of list[float]，与 arrays 等长
///
/// 示例:
/// ```python
/// close_arrays = [df1['close'].tolist(), df2['close'].tolist(), ...]
/// results = parallel_apply(close_arrays, 'ma', period=20)
/// ```
#[pyfunction]
#[pyo3(signature = (arrays, func_name, period=5))]
pub fn parallel_apply(
    arrays: Vec<Vec<f64>>,
    func_name: String,
    period: u32,
) -> Vec<Vec<f64>> {
    arrays.into_par_iter().map(|v| {
        match func_name.to_lowercase().as_str() {
            "ma"         => crate::qapyo3::ma(v, period),
            "ema"        => crate::qapyo3::ema(v, period),
            "rsi"        => crate::qapyo3::rsi(v, period),
            "roc"        => crate::qapyo3::roc(v, period),
            "momentum"   => crate::qapyo3::momentum(v, period),
            "volatility" => crate::qapyo3::volatility(v, period),
            "hhv"        => crate::qapyo3::hhv(v, period),
            "llv"        => crate::qapyo3::llv(v, period),
            "returns"    => crate::qapyo3::returns(v),
            _            => vec![f64::NAN; v.len()],
        }
    }).collect()
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PyQAAccount（保留原有实现）
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Python 侧的 QIFI 账户（线程安全包装）
///
/// 兼容 `qars3.QA_QIFIAccount` API，可无缝替换。
#[pyclass(name = "PyQAAccount")]
pub struct PyQAAccount {
    inner: Arc<Mutex<QA_Account>>,
}

#[pymethods]
impl PyQAAccount {
    #[new]
    #[pyo3(signature = (account_cookie, portfolio_cookie="default", init_cash=1_000_000.0, environment="backtest"))]
    pub fn new(
        account_cookie: &str,
        portfolio_cookie: &str,
        init_cash: f64,
        environment: &str,
    ) -> Self {
        let acc = QA_Account::new(
            account_cookie, portfolio_cookie, "admin", init_cash, false, environment,
        );
        PyQAAccount { inner: Arc::new(Mutex::new(acc)) }
    }

    pub fn buy(&self, code: &str, price: f64, datetime: &str, amount: f64) -> bool {
        self.inner.lock().unwrap().buy(code, amount, datetime, price).is_ok()
    }

    pub fn sell(&self, code: &str, price: f64, datetime: &str, amount: f64) -> bool {
        self.inner.lock().unwrap().sell(code, amount, datetime, price).is_ok()
    }

    pub fn buy_open(&self, code: &str, price: f64, datetime: &str, amount: f64) -> bool {
        self.inner.lock().unwrap().buy_open(code, amount, datetime, price).is_ok()
    }

    pub fn sell_open(&self, code: &str, price: f64, datetime: &str, amount: f64) -> bool {
        self.inner.lock().unwrap().sell_open(code, amount, datetime, price).is_ok()
    }

    pub fn buy_close(&self, code: &str, price: f64, datetime: &str, amount: f64) -> bool {
        self.inner.lock().unwrap().buy_close(code, amount, datetime, price).is_ok()
    }

    pub fn sell_close(&self, code: &str, price: f64, datetime: &str, amount: f64) -> bool {
        self.inner.lock().unwrap().sell_close(code, amount, datetime, price).is_ok()
    }

    pub fn buy_closetoday(&self, code: &str, price: f64, datetime: &str, amount: f64) -> bool {
        self.inner.lock().unwrap().buy_closetoday(code, amount, datetime, price).is_ok()
    }

    pub fn sell_closetoday(&self, code: &str, price: f64, datetime: &str, amount: f64) -> bool {
        self.inner.lock().unwrap().sell_closetoday(code, amount, datetime, price).is_ok()
    }

    pub fn on_price_change(&self, code: &str, price: f64, datetime: &str) {
        self.inner.lock().unwrap().on_price_change(code.to_string(), price, datetime.to_string());
    }

    pub fn settle(&self) {
        self.inner.lock().unwrap().settle();
    }

    pub fn get_balance(&self) -> f64 {
        self.inner.lock().unwrap().get_balance()
    }

    pub fn get_volume_long(&self, code: &str) -> f64 {
        self.inner.lock().unwrap().get_volume_long(code)
    }

    pub fn get_volume_short(&self, code: &str) -> f64 {
        self.inner.lock().unwrap().get_volume_short(code)
    }

    pub fn get_qifi(&self, py: Python<'_>) -> PyObject {
        let mut acc = self.inner.lock().unwrap();
        let qifi = acc.get_qifi_slice();
        let json_str = serde_json::to_string(&qifi).unwrap_or_else(|_| "{}".to_string());
        let json_val: serde_json::Value = serde_json::from_str(&json_str)
            .unwrap_or(serde_json::Value::Object(Default::default()));
        serde_value_to_pyobject(py, &json_val)
    }

    pub fn get_account_info(&self, py: Python<'_>) -> PyObject {
        let mut acc = self.inner.lock().unwrap();
        let qifi = acc.get_qifi_slice();
        let json_str = serde_json::to_string(&qifi.accounts).unwrap_or_else(|_| "{}".to_string());
        let json_val: serde_json::Value = serde_json::from_str(&json_str)
            .unwrap_or(serde_json::Value::Object(Default::default()));
        serde_value_to_pyobject(py, &json_val)
    }

    fn __repr__(&self) -> String {
        let mut acc = self.inner.lock().unwrap();
        let bal = acc.get_balance();
        let cookie = acc.account_cookie.clone();
        format!("PyQAAccount(cookie='{}', balance={:.2})", cookie, bal)
    }
}

// ─── JSON → PyObject ────────────────────────────────────────────────────────

fn serde_value_to_pyobject(py: Python<'_>, val: &serde_json::Value) -> PyObject {
    match val {
        serde_json::Value::Null => py.None(),
        serde_json::Value::Bool(b) => b.into_py(py),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { i.into_py(py) }
            else if let Some(f) = n.as_f64() { f.into_py(py) }
            else { py.None() }
        }
        serde_json::Value::String(s) => s.clone().into_py(py),
        serde_json::Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr { list.append(serde_value_to_pyobject(py, item)).ok(); }
            list.into_py(py)
        }
        serde_json::Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map { dict.set_item(k, serde_value_to_pyobject(py, v)).ok(); }
            dict.into_py(py)
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 模块入口
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[pymodule]
pub fn qapro_rs_module(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    // 类
    m.add_class::<PyQAAccount>()?;
    m.add_class::<PyQADataFrame>()?;

    // 指标函数
    m.add_function(wrap_pyfunction!(ma,          m)?)?;
    m.add_function(wrap_pyfunction!(ema,         m)?)?;
    m.add_function(wrap_pyfunction!(macd,        m)?)?;
    m.add_function(wrap_pyfunction!(rsi,         m)?)?;
    m.add_function(wrap_pyfunction!(boll,        m)?)?;
    m.add_function(wrap_pyfunction!(atr,         m)?)?;
    m.add_function(wrap_pyfunction!(kdj,         m)?)?;
    m.add_function(wrap_pyfunction!(obv,         m)?)?;
    m.add_function(wrap_pyfunction!(hhv,         m)?)?;
    m.add_function(wrap_pyfunction!(llv,         m)?)?;
    m.add_function(wrap_pyfunction!(roc,         m)?)?;
    m.add_function(wrap_pyfunction!(momentum,    m)?)?;
    m.add_function(wrap_pyfunction!(returns,     m)?)?;
    m.add_function(wrap_pyfunction!(volatility,  m)?)?;
    m.add_function(wrap_pyfunction!(correlation, m)?)?;

    // Arrow / 并行
    m.add_function(wrap_pyfunction!(from_arrow,      m)?)?;
    m.add_function(wrap_pyfunction!(process_arrow,   m)?)?;
    m.add_function(wrap_pyfunction!(parallel_apply,  m)?)?;

    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

// ─── 测试 ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_account() -> PyQAAccount {
        PyQAAccount {
            inner: Arc::new(Mutex::new(QA_Account::new(
                "test_pyo3", "default", "admin", 100_000.0, false, "backtest",
            ))),
        }
    }

    #[test]
    fn test_new_account_balance() {
        let acc = make_account();
        assert_eq!(acc.get_balance(), 100_000.0);
    }

    #[test]
    fn test_buy_stock() {
        let acc = make_account();
        assert!(acc.buy("000001", 10.0, "2025-01-15", 100.0));
        assert!(acc.get_volume_long("000001") > 0.0);
    }

    #[test]
    fn test_indicator_ma() {
        let v: Vec<f64> = (1..=20).map(|x| x as f64).collect();
        let result = ma(v.clone(), 5);
        assert_eq!(result.len(), 20);
        // MA5 of [1..5] = 3.0
        assert!((result[4] - 3.0).abs() < 1e-9, "MA5 at index 4 should be 3.0");
        // MA5 of [16..20] = 18.0
        assert!((result[19] - 18.0).abs() < 1e-9, "MA5 at index 19 should be 18.0");
    }

    #[test]
    fn test_indicator_returns() {
        let v = vec![100.0, 110.0, 99.0];
        let r = returns(v);
        assert!(r[0].is_nan());
        assert!((r[1] - 0.1).abs() < 1e-9);
        assert!((r[2] - (-0.1)).abs() < 1e-9);
    }

    #[test]
    fn test_parallel_apply_ma() {
        let arrays = vec![
            (1..=20).map(|x| x as f64).collect::<Vec<f64>>(),
            (10..=30).map(|x| x as f64 * 2.0).collect::<Vec<f64>>(),
        ];
        let results = parallel_apply(arrays, "ma".to_string(), 5);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].len(), 20);
        assert_eq!(results[1].len(), 21);
    }

    #[test]
    fn test_correlation() {
        let x: Vec<f64> = (1..=20).map(|i| i as f64).collect();
        let y: Vec<f64> = x.clone(); // 完全正相关
        let r = correlation(x, y, 5);
        // 完全正相关应为 1.0（跳过预热期的 NaN）
        for &v in r.iter().skip(4) {
            assert!((v - 1.0).abs() < 1e-9, "完全正相关应为 1.0，got {}", v);
        }
    }
}
