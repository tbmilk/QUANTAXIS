"""
QUANTAXIS/QARSBridge —— Rust 高性能组件桥接层

提供与 qars2 完全兼容的 Python API：
  - QARSAccount:  Rust 高性能 QIFI 账户（100x 加速）
  - QARSBacktest: Rust 回测引擎（10x 加速）
  - QADataFrame:  高性能数据帧（指标计算、Arrow 转换）
  - ma/ema/macd/rsi/boll/atr/kdj/obv 等向量化指标函数
  - parallel_apply: 多标的并行指标计算
  - from_arrow/process_arrow: PyArrow 互操作

使用示例：
    >>> from QUANTAXIS.QARSBridge import ma, ema, macd, QADataFrame
    >>>
    >>> ma5 = ma(close_list, 5)                    # list[float]
    >>> result = macd(close_list)                  # dict(macd/signal/histogram)
    >>>
    >>> df = QADataFrame(stock_df)                 # pandas DataFrame
    >>> df.boll(20)                                # dict(upper/mid/lower)
    >>>
    >>> from QUANTAXIS.QARSBridge import QARSAccount
    >>> account = QARSAccount("my_account", init_cash=1_000_000)
    >>> account.buy("000001", 10.5, "2025-01-01", 100)

@yutiansut @quantaxis
"""

__version__ = "2.1.0.alpha2"

# ──────────────────────────────────────────────────────────────────────────────
# 检测 Rust 后端
# ──────────────────────────────────────────────────────────────────────────────
try:
    import qapro_rs as _qapro_rs
    HAS_QARS = True
    QARS_VERSION = getattr(_qapro_rs, '__version__', 'unknown')
except ImportError:
    _qapro_rs = None
    HAS_QARS = False
    QARS_VERSION = None

# 兼容旧版 qars3 检测
_has_qars3 = False
try:
    import qars3 as _qars3
    _has_qars3 = True
    if not HAS_QARS:
        HAS_QARS = True
        QARS_VERSION = getattr(_qars3, '__version__', 'unknown')
except ImportError:
    _qars3 = None


def has_qars_support() -> bool:
    """检查是否有 Rust 核心支持（qapro_rs 或 qars3）"""
    return HAS_QARS


def get_version_info() -> dict:
    """获取 QARS 桥接层版本信息"""
    return {
        'bridge_version': __version__,
        'has_qars': HAS_QARS,
        'qars_version': QARS_VERSION,
        'backend': 'Rust' if HAS_QARS else 'Python',
        'has_qapro_rs': _qapro_rs is not None,
        'has_qars3': _has_qars3,
    }


# ──────────────────────────────────────────────────────────────────────────────
# 数据处理 API（向量化指标、QADataFrame、Arrow、并行）
# ──────────────────────────────────────────────────────────────────────────────
from .qars_data import (    # noqa: E402, F401
    QADataFrame,
    ma, ema, macd, rsi, boll, atr, kdj, obv,
    hhv, llv, roc, momentum, returns, volatility, correlation,
    parallel_apply,
    from_arrow, process_arrow,
)

# ──────────────────────────────────────────────────────────────────────────────
# 延迟加载重量级/可选组件
# ──────────────────────────────────────────────────────────────────────────────
def _load_qars_account():
    if _has_qars3:
        from .qars_account import QARSAccount
        return QARSAccount
    if _qapro_rs is not None:
        from .qars_account_rs import QARSAccount
        return QARSAccount
    try:
        from ..QIFI.QifiAccount import QIFI_Account
    except ImportError as exc:
        raise ImportError(
            "QARSAccount Python 回退实现依赖 QIFI/pandas/pymongo 等组件。"
            " 如仅需桥接层，请安装 quantaxis 基础依赖；如需 Rust 后端，请安装 quantaxis[rust]。"
        ) from exc
    return QIFI_Account


def _load_qars_backtest():
    if _has_qars3:
        from .qars_backtest import QARSBacktest
        return QARSBacktest

    class _UnavailableQARSBacktest:
        def __init__(self, *args, **kwargs):
            raise ImportError("QARSBacktest 需要 qars3。安装: pip install quantaxis[rust]")

    return _UnavailableQARSBacktest


def _load_risk_event_consumer():
    from .redis_consumer import RiskEventConsumer
    return RiskEventConsumer


def __getattr__(name):
    if name == 'QARSAccount':
        value = _load_qars_account()
    elif name == 'QARSBacktest':
        value = _load_qars_backtest()
    elif name == 'RiskEventConsumer':
        value = _load_risk_event_consumer()
    else:
        raise AttributeError(f"module '{__name__}' has no attribute '{name}'")
    globals()[name] = value
    return value

# ──────────────────────────────────────────────────────────────────────────────
__all__ = [
    # 账户与回测
    'QARSAccount',
    'QARSBacktest',
    # 数据帧
    'QADataFrame',
    # 向量化指标
    'ma', 'ema', 'macd', 'rsi', 'boll', 'atr', 'kdj', 'obv',
    'hhv', 'llv', 'roc', 'momentum', 'returns', 'volatility', 'correlation',
    # 并行与 Arrow
    'parallel_apply', 'from_arrow', 'process_arrow',
    # 工具
    'RiskEventConsumer',
    'has_qars_support', 'get_version_info',
    'HAS_QARS', 'QARS_VERSION',
]
