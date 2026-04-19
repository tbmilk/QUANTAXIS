pub mod qaaccount;
pub mod qaconnector;
pub mod qadata;
pub mod qaenv;
pub mod qamarket;
pub mod qaprotocol;
pub mod qapubsub;
pub mod qaruntime;
pub mod qautil;

pub mod qafuncs;
pub mod qalog;
pub mod qamacros;
pub mod qapraser;
pub mod qastrategy;

pub mod qadatastruct;
pub mod qafactor;
pub mod qahandlers;

pub mod parsers;

pub mod qarisk;
pub mod qaportfolio;
pub mod qaexec;

pub mod qaindicator;
pub mod qatrader;

// ─── PyO3 Python 绑定（可选 feature） ────────────────────────────────────────
#[cfg(feature = "pyo3-bindings")]
pub mod qapyo3;

#[cfg(feature = "pyo3-bindings")]
use pyo3::prelude::*;

/// Python 模块入口（`maturin develop --features pyo3-bindings`）
#[cfg(feature = "pyo3-bindings")]
#[pymodule]
fn qapro_rs(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    qapyo3::qapro_rs_module(_py, m)
}
