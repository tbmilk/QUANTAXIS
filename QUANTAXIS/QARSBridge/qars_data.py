"""
qars_data.py —— 数据处理 API，贴近 qars2 接口

优先使用 qapro_rs（Rust）实现；不可用时自动降级到纯 Python + pandas/numpy。

公开 API（与 qars2 完全兼容）：
    ma(values, period=5)
    ema(values, period=12)
    macd(values, fast=12, slow=26, signal=9)
    rsi(values, period=14)
    boll(values, period=20, std_dev=2.0)
    atr(high, low, close, period=14)
    kdj(high, low, close, n=9, m1=3, m2=3)
    obv(close, volume)
    hhv(values, period=5)
    llv(values, period=5)
    roc(values, period=12)
    momentum(values, period=12)
    returns(values)
    volatility(values, period=20)
    correlation(x, y, period=20)
    parallel_apply(arrays, func_name, period=5)
    from_arrow(table)
    process_arrow(table, ops=None)
    QADataFrame(data)
"""

from typing import Dict, List, Optional, Union, Any
import math

# ──────────────────────────────────────────────────────────────────────────────
# 检测 Rust 后端
# ──────────────────────────────────────────────────────────────────────────────
try:
    import qapro_rs as _rs
    _HAS_RS = True
except ImportError:
    _rs = None
    _HAS_RS = False

# ──────────────────────────────────────────────────────────────────────────────
# 纯 Python 实现（fallback）
# ──────────────────────────────────────────────────────────────────────────────

def _to_list(values) -> List[float]:
    """统一转换为 Python list[float]"""
    if hasattr(values, 'tolist'):
        return values.tolist()
    return list(values)


def _py_ma(values: List[float], period: int = 5) -> List[float]:
    n = len(values)
    result = [float('nan')] * n
    for i in range(period - 1, n):
        result[i] = sum(values[i - period + 1: i + 1]) / period
    return result


def _py_ema(values: List[float], period: int = 12) -> List[float]:
    n = len(values)
    result = [float('nan')] * n
    k = 2.0 / (period + 1)
    for i in range(n):
        if i == 0:
            result[i] = values[i]
        else:
            prev = result[i - 1] if not math.isnan(result[i - 1]) else values[i]
            result[i] = values[i] * k + prev * (1 - k)
    return result


def _py_macd(values: List[float], fast: int = 12, slow: int = 26,
             signal: int = 9) -> Dict[str, List[float]]:
    ema_fast = _py_ema(values, fast)
    ema_slow = _py_ema(values, slow)
    n = len(values)
    macd_line = [float('nan')] * n
    sig_line  = [float('nan')] * n
    hist      = [float('nan')] * n
    for i in range(n):
        if not (math.isnan(ema_fast[i]) or math.isnan(ema_slow[i])):
            macd_line[i] = ema_fast[i] - ema_slow[i]
    # Signal line = EMA(macd_line)
    valid = [(i, v) for i, v in enumerate(macd_line) if not math.isnan(v)]
    if valid:
        k = 2.0 / (signal + 1)
        for idx, (i, v) in enumerate(valid):
            if idx == 0:
                sig_line[i] = v
            else:
                prev_i = valid[idx - 1][0]
                sig_line[i] = v * k + sig_line[prev_i] * (1 - k)
            hist[i] = macd_line[i] - sig_line[i]
    return {'macd': macd_line, 'signal': sig_line, 'histogram': hist}


def _py_rsi(values: List[float], period: int = 14) -> List[float]:
    n = len(values)
    result = [float('nan')] * n
    gains, losses = 0.0, 0.0
    for i in range(1, n):
        change = values[i] - values[i - 1]
        gain = change if change > 0 else 0.0
        loss = -change if change < 0 else 0.0
        if i < period:
            gains += gain
            losses += loss
        elif i == period:
            gains = (gains + gain) / period
            losses = (losses + loss) / period
            result[i] = 100.0 - 100.0 / (1.0 + gains / losses) if losses != 0 else 100.0
        else:
            gains = (gains * (period - 1) + gain) / period
            losses = (losses * (period - 1) + loss) / period
            result[i] = 100.0 - 100.0 / (1.0 + gains / losses) if losses != 0 else 100.0
    return result


def _py_boll(values: List[float], period: int = 20,
             std_dev: float = 2.0) -> Dict[str, List[float]]:
    n = len(values)
    upper = [float('nan')] * n
    mid   = [float('nan')] * n
    lower = [float('nan')] * n
    for i in range(period - 1, n):
        window = values[i - period + 1: i + 1]
        m = sum(window) / period
        s = (sum((v - m) ** 2 for v in window) / period) ** 0.5
        mid[i]   = m
        upper[i] = m + std_dev * s
        lower[i] = m - std_dev * s
    return {'upper': upper, 'mid': mid, 'lower': lower}


def _py_atr(high: List[float], low: List[float], close: List[float],
            period: int = 14) -> List[float]:
    n = len(close)
    tr_list = [float('nan')] * n
    for i in range(1, n):
        hl = high[i] - low[i]
        hc = abs(high[i] - close[i - 1])
        lc = abs(low[i] - close[i - 1])
        tr_list[i] = max(hl, hc, lc)
    result = [float('nan')] * n
    if n > period:
        result[period] = sum(tr_list[1: period + 1]) / period
        for i in range(period + 1, n):
            if not math.isnan(tr_list[i]) and not math.isnan(result[i - 1]):
                result[i] = (result[i - 1] * (period - 1) + tr_list[i]) / period
    return result


def _py_kdj(high: List[float], low: List[float], close: List[float],
            n: int = 9, m1: int = 3, m2: int = 3) -> Dict[str, List[float]]:
    total = len(close)
    k_v = [50.0] * total
    d_v = [50.0] * total
    j_v = [50.0] * total
    for i in range(n - 1, total):
        h = max(high[i - n + 1: i + 1])
        l = min(low[i - n + 1: i + 1])
        rsv = (close[i] - l) / (h - l) * 100 if h != l else 50.0
        k_v[i] = (2 * k_v[i - 1] + rsv) / m1
        d_v[i] = (2 * d_v[i - 1] + k_v[i]) / m2
        j_v[i] = 3 * k_v[i] - 2 * d_v[i]
    for i in range(n - 1):
        k_v[i] = float('nan')
        d_v[i] = float('nan')
        j_v[i] = float('nan')
    return {'k': k_v, 'd': d_v, 'j': j_v}


def _py_obv(close: List[float], volume: List[float]) -> List[float]:
    n = len(close)
    result = [0.0] * n
    for i in range(1, n):
        if close[i] > close[i - 1]:
            result[i] = result[i - 1] + volume[i]
        elif close[i] < close[i - 1]:
            result[i] = result[i - 1] - volume[i]
        else:
            result[i] = result[i - 1]
    return result


def _py_hhv(values: List[float], period: int = 5) -> List[float]:
    n = len(values)
    result = [float('nan')] * n
    for i in range(period - 1, n):
        result[i] = max(values[i - period + 1: i + 1])
    return result


def _py_llv(values: List[float], period: int = 5) -> List[float]:
    n = len(values)
    result = [float('nan')] * n
    for i in range(period - 1, n):
        result[i] = min(values[i - period + 1: i + 1])
    return result


def _py_roc(values: List[float], period: int = 12) -> List[float]:
    n = len(values)
    result = [float('nan')] * n
    for i in range(period, n):
        base = values[i - period]
        result[i] = (values[i] - base) / base * 100 if base != 0 else float('nan')
    return result


def _py_returns(values: List[float]) -> List[float]:
    n = len(values)
    result = [float('nan')] * n
    for i in range(1, n):
        base = values[i - 1]
        result[i] = (values[i] - base) / base if base != 0 else float('nan')
    return result


def _py_volatility(values: List[float], period: int = 20) -> List[float]:
    n = len(values)
    result = [float('nan')] * n
    for i in range(period - 1, n):
        window = values[i - period + 1: i + 1]
        m = sum(window) / period
        s = (sum((v - m) ** 2 for v in window) / period) ** 0.5
        result[i] = s
    return result


def _py_correlation(x: List[float], y: List[float], period: int = 20) -> List[float]:
    n = len(x)
    result = [float('nan')] * n
    for i in range(period - 1, n):
        xs = x[i - period + 1: i + 1]
        ys = y[i - period + 1: i + 1]
        mx = sum(xs) / period
        my = sum(ys) / period
        cov = sum((a - mx) * (b - my) for a, b in zip(xs, ys)) / period
        sx = (sum((a - mx) ** 2 for a in xs) / period) ** 0.5
        sy = (sum((b - my) ** 2 for b in ys) / period) ** 0.5
        result[i] = cov / (sx * sy) if sx * sy != 0 else float('nan')
    return result


# ──────────────────────────────────────────────────────────────────────────────
# 统一公开 API（自动路由到 Rust 或 Python）
# ──────────────────────────────────────────────────────────────────────────────

def ma(values, period: int = 5) -> List[float]:
    """简单移动平均 MA(n)"""
    v = _to_list(values)
    return _rs.ma(v, period) if _HAS_RS else _py_ma(v, period)


def ema(values, period: int = 12) -> List[float]:
    """指数移动平均 EMA(n)"""
    v = _to_list(values)
    return _rs.ema(v, period) if _HAS_RS else _py_ema(v, period)


def macd(values, fast: int = 12, slow: int = 26, signal: int = 9) -> Dict[str, List[float]]:
    """MACD — 返回 dict(macd, signal, histogram)"""
    v = _to_list(values)
    return _rs.macd(v, fast, slow, signal) if _HAS_RS else _py_macd(v, fast, slow, signal)


def rsi(values, period: int = 14) -> List[float]:
    """相对强弱指标 RSI(n)"""
    v = _to_list(values)
    return _rs.rsi(v, period) if _HAS_RS else _py_rsi(v, period)


def boll(values, period: int = 20, std_dev: float = 2.0) -> Dict[str, List[float]]:
    """布林带 — 返回 dict(upper, mid, lower)"""
    v = _to_list(values)
    return _rs.boll(v, period, std_dev) if _HAS_RS else _py_boll(v, period, std_dev)


def atr(high, low, close, period: int = 14) -> List[float]:
    """平均真实波幅 ATR(n)"""
    h, l, c = _to_list(high), _to_list(low), _to_list(close)
    return _rs.atr(h, l, c, period) if _HAS_RS else _py_atr(h, l, c, period)


def kdj(high, low, close, n: int = 9, m1: int = 3, m2: int = 3) -> Dict[str, List[float]]:
    """KDJ 随机指标 — 返回 dict(k, d, j)"""
    h, l, c = _to_list(high), _to_list(low), _to_list(close)
    return _rs.kdj(h, l, c, n, m1, m2) if _HAS_RS else _py_kdj(h, l, c, n, m1, m2)


def obv(close, volume) -> List[float]:
    """能量潮 OBV"""
    c, v = _to_list(close), _to_list(volume)
    return _rs.obv(c, v) if _HAS_RS else _py_obv(c, v)


def hhv(values, period: int = 5) -> List[float]:
    """最高值 HHV(n)"""
    v = _to_list(values)
    return _rs.hhv(v, period) if _HAS_RS else _py_hhv(v, period)


def llv(values, period: int = 5) -> List[float]:
    """最低值 LLV(n)"""
    v = _to_list(values)
    return _rs.llv(v, period) if _HAS_RS else _py_llv(v, period)


def roc(values, period: int = 12) -> List[float]:
    """变动率 ROC(n)"""
    v = _to_list(values)
    return _rs.roc(v, period) if _HAS_RS else _py_roc(v, period)


def momentum(values, period: int = 12) -> List[float]:
    """价格动量（ROC 的小数版，与 qars2.momentum 一致）"""
    v = _to_list(values)
    return _rs.momentum(v, period) if _HAS_RS else [x / 100.0 if not math.isnan(x) else x
                                                     for x in _py_roc(v, period)]


def returns(values) -> List[float]:
    """简单收益率（逐日）"""
    v = _to_list(values)
    return _rs.returns(v) if _HAS_RS else _py_returns(v)


def volatility(values, period: int = 20) -> List[float]:
    """滚动波动率（滚动标准差）"""
    v = _to_list(values)
    return _rs.volatility(v, period) if _HAS_RS else _py_volatility(v, period)


def correlation(x, y, period: int = 20) -> List[float]:
    """滚动相关系数"""
    xv, yv = _to_list(x), _to_list(y)
    return _rs.correlation(xv, yv, period) if _HAS_RS else _py_correlation(xv, yv, period)


def parallel_apply(arrays: List, func_name: str, period: int = 5) -> List[List[float]]:
    """对多个价格序列并行应用同一指标（Rust 版自动使用 Rayon 并行）"""
    converted = [_to_list(a) for a in arrays]
    if _HAS_RS:
        return _rs.parallel_apply(converted, func_name, period)
    # Python fallback：顺序执行
    _func_map = {
        'ma': lambda v: _py_ma(v, period),
        'ema': lambda v: _py_ema(v, period),
        'rsi': lambda v: _py_rsi(v, period),
        'roc': lambda v: _py_roc(v, period),
        'momentum': lambda v: [x / 100.0 if not math.isnan(x) else x for x in _py_roc(v, period)],
        'volatility': lambda v: _py_volatility(v, period),
        'hhv': lambda v: _py_hhv(v, period),
        'llv': lambda v: _py_llv(v, period),
        'returns': lambda v: _py_returns(v),
    }
    func = _func_map.get(func_name.lower())
    if func is None:
        return [[float('nan')] * len(a) for a in converted]
    return [func(a) for a in converted]


def from_arrow(table) -> 'QADataFrame':
    """从 PyArrow Table 创建 QADataFrame"""
    if _HAS_RS:
        return _rs.QADataFrame.from_arrow(table)
    data = table.to_pydict()
    return QADataFrame(data)


def process_arrow(table, ops: Optional[List[str]] = None):
    """处理 PyArrow Table，返回带指标的 dict（贴近 qars2.process_arrow）"""
    if _HAS_RS:
        return _rs.process_arrow(table, ops)
    df = QADataFrame(table.to_pydict())
    return df._apply_ops(ops)


# ──────────────────────────────────────────────────────────────────────────────
# QADataFrame —— 贴近 qars2.QADataFrame
# ──────────────────────────────────────────────────────────────────────────────

class QADataFrame:
    """
    高性能数据帧，与 qars2.QADataFrame API 兼容。

    Rust 可用时直接使用 qapro_rs.QADataFrame；否则使用纯 Python 实现。

    示例:
        >>> import pandas as pd
        >>> from QUANTAXIS.QARSBridge import QADataFrame
        >>>
        >>> df = QADataFrame(stock_df)          # pandas DataFrame
        >>> ma5  = df.ma(5)                      # list[float]
        >>> macd = df.macd()                     # dict(macd, signal, histogram)
        >>> boll = df.boll()                     # dict(upper, mid, lower)
        >>> df.to_dataframe()                    # 转回 pandas DataFrame
    """

    def __init__(self, data):
        """
        Args:
            data: pandas DataFrame、dict 或 qapro_rs.QADataFrame
        """
        if _HAS_RS and isinstance(data, _rs.QADataFrame):
            self._rs_df = data
            self._py_data = None
        elif _HAS_RS:
            self._rs_df = _rs.QADataFrame(data)
            self._py_data = None
        else:
            self._rs_df = None
            # 转换为 dict[str, list[float]]
            if hasattr(data, 'to_dict'):
                self._py_data = {k: _to_list(v) for k, v in data.to_dict('list').items()}
            else:
                self._py_data = {k: _to_list(v) for k, v in dict(data).items()}

    def _close(self) -> List[float]:
        if self._rs_df is None:
            return self._py_data.get('close', [])
        return self._rs_df.to_dict().get('close', [])

    def _col(self, name: str) -> List[float]:
        if self._rs_df is None:
            return self._py_data.get(name, [])
        return self._rs_df.to_dict().get(name, [])

    def ma(self, period: int = 5, col: str = 'close') -> List[float]:
        if self._rs_df is not None:
            return self._rs_df.ma(period, col)
        return _py_ma(self._py_data.get(col, []), period)

    def ema(self, period: int = 12, col: str = 'close') -> List[float]:
        if self._rs_df is not None:
            return self._rs_df.ema(period, col)
        return _py_ema(self._py_data.get(col, []), period)

    def macd(self, fast: int = 12, slow: int = 26, signal: int = 9) -> Dict:
        if self._rs_df is not None:
            return self._rs_df.macd(fast, slow, signal)
        return _py_macd(self._close(), fast, slow, signal)

    def rsi(self, period: int = 14) -> List[float]:
        if self._rs_df is not None:
            return self._rs_df.rsi(period)
        return _py_rsi(self._close(), period)

    def boll(self, period: int = 20, std_dev: float = 2.0) -> Dict:
        if self._rs_df is not None:
            return self._rs_df.boll(period, std_dev)
        return _py_boll(self._close(), period, std_dev)

    def atr(self, period: int = 14) -> List[float]:
        if self._rs_df is not None:
            return self._rs_df.atr(period)
        return _py_atr(self._col('high'), self._col('low'), self._close(), period)

    def kdj(self, n: int = 9, m1: int = 3, m2: int = 3) -> Dict:
        if self._rs_df is not None:
            return self._rs_df.kdj(n, m1, m2)
        return _py_kdj(self._col('high'), self._col('low'), self._close(), n, m1, m2)

    def obv(self) -> List[float]:
        if self._rs_df is not None:
            return self._rs_df.obv()
        return _py_obv(self._close(), self._col('volume'))

    def momentum(self, period: int = 12) -> List[float]:
        if self._rs_df is not None:
            return self._rs_df.momentum(period)
        return [x / 100.0 if not math.isnan(x) else x for x in _py_roc(self._close(), period)]

    def returns(self) -> List[float]:
        if self._rs_df is not None:
            return self._rs_df.returns()
        return _py_returns(self._close())

    def volatility(self, period: int = 20) -> List[float]:
        if self._rs_df is not None:
            return self._rs_df.volatility(period)
        return _py_volatility(self._close(), period)

    def to_dict(self) -> Dict[str, List[float]]:
        if self._rs_df is not None:
            return self._rs_df.to_dict()
        return dict(self._py_data)

    def to_dataframe(self):
        """转换回 pandas DataFrame"""
        import pandas as pd
        return pd.DataFrame(self.to_dict())

    def to_arrow(self):
        """转换为 PyArrow Table"""
        if self._rs_df is not None:
            return self._rs_df.to_arrow()
        import pyarrow as pa
        return pa.table(self.to_dict())

    def _apply_ops(self, ops=None):
        """内部：process_arrow 的 Python fallback"""
        result = dict(self.to_dict())
        ops = ops or ['ma5', 'ma10', 'ma20', 'ema12', 'ema26', 'macd', 'rsi14']
        for op in ops:
            import re
            m = re.match(r'([a-z]+)(\d*)', op.lower())
            if not m:
                continue
            name, p_str = m.group(1), m.group(2)
            p = int(p_str) if p_str else 0
            if name == 'ma':
                p = p or 5
                result[f'ma{p}'] = _py_ma(self._close(), p)
            elif name == 'ema':
                p = p or 12
                result[f'ema{p}'] = _py_ema(self._close(), p)
            elif name == 'macd':
                result['macd'] = _py_macd(self._close())
            elif name == 'rsi':
                p = p or 14
                result[f'rsi{p}'] = _py_rsi(self._close(), p)
            elif name == 'boll':
                p = p or 20
                result['boll'] = _py_boll(self._close(), p)
        return result

    def __len__(self) -> int:
        if self._rs_df is not None:
            return len(self._rs_df)
        return len(next(iter(self._py_data.values()), []))

    def __repr__(self) -> str:
        backend = 'Rust' if self._rs_df is not None else 'Python'
        return f"QADataFrame(rows={len(self)}, backend='{backend}')"
