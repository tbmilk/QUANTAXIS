# coding:utf-8

from __future__ import annotations

import datetime
from datetime import date

import pandas as pd
from retrying import retry

from QUANTAXIS.QAFetch.base import (
    _select_bond_market_code,
    _select_index_code,
    _select_market_code,
)
from QUANTAXIS.QAUtil import (
    QA_util_date_stamp,
    QA_util_date_str2int,
    QA_util_get_real_datelist,
    QA_util_get_trade_gap,
    QA_util_log_info,
    QA_util_time_stamp,
    trade_date_sse,
)

try:
    from opentdx import (
        BLOCK_FILE_TYPE,
        BOARD_TYPE,
        EX_BOARD_TYPE,
        EX_MARKET,
        MARKET,
        PERIOD,
        TdxClient,
    )
    try:
        from opentdx import QuotationClient
    except ImportError:  # pragma: no cover - depends on opentdx version
        from opentdx.client.standardClient import StandardClient as QuotationClient
    _OPENTDX_IMPORT_ERROR = None
except Exception as e:  # pragma: no cover - depends on local env
    TdxClient = None
    MARKET = None
    EX_MARKET = None
    PERIOD = None
    QuotationClient = None
    BLOCK_FILE_TYPE = None
    BOARD_TYPE = None
    EX_BOARD_TYPE = None
    _OPENTDX_IMPORT_ERROR = e


_XDXR_CATEGORY_NAME = {
    "除权除息": 1,
    "送配股上市": 2,
    "非流通股上市": 3,
    "未知股本变动": 4,
    "股本变化": 5,
    "增发新股": 6,
    "股份回购": 7,
    "增发新股上市": 8,
    "转配股上市": 9,
    "可转债上市": 10,
    "扩缩股": 11,
    "非流通股缩股": 12,
    "送认购权证": 13,
    "送认沽权证": 14,
}

_STANDARD_BLOCK_TYPES = [
    (BLOCK_FILE_TYPE.DEFAULT, "yb"),
    (BLOCK_FILE_TYPE.FG, "fg"),
    (BLOCK_FILE_TYPE.GN, "gn"),
    (BLOCK_FILE_TYPE.ZS, "zs"),
    (BLOCK_FILE_TYPE.HK, "hk"),
    (BLOCK_FILE_TYPE.JJ, "jj"),
] if BLOCK_FILE_TYPE else []

extension_market_list = None
goods_varieties_list = None
_VALID_EX_MARKET_VALUES = {item.value for item in EX_MARKET} if EX_MARKET else set()


def _is_day_frequency(freq: str) -> bool:
    return str(freq) in ["day", "d", "D", "DAY", "Day", "w", "W", "Week", "week", "month", "M", "m", "Month", "quarter", "Q", "Quarter", "q", "y", "Y", "year", "Year"]


def _ensure_opentdx():
    if TdxClient is None:
        raise RuntimeError(
            "opentdx import failed. Install with "
            "`pip install 'quantaxis[opentdx]'` on Python >=3.12 "
            f"or install `opentdx` directly. Original error: {_OPENTDX_IMPORT_ERROR}"
        )


def _market_from_int(value: int) -> MARKET:
    return MARKET.SH if int(value) == 1 else MARKET.SZ


def _stock_market(code: str) -> MARKET:
    return _market_from_int(_select_market_code(code))


def _bond_market(code: str) -> MARKET:
    return _market_from_int(_select_bond_market_code(code))


def _index_market(code: str) -> MARKET:
    return _market_from_int(_select_index_code(code))


def _index_uses_stock_channel(code: str) -> bool:
    return str(code)[0] in ["5", "1"]


def _period_from_day(freq: str) -> PERIOD:
    text = str(freq)
    if text in ["day", "d", "D", "DAY", "Day"]:
        return PERIOD.DAILY
    if text in ["w", "W", "Week", "week"]:
        return PERIOD.WEEKLY
    if text in ["month", "M", "m", "Month"]:
        return PERIOD.MONTHLY
    if text in ["quarter", "Q", "Quarter", "q"]:
        return PERIOD.QUARTERLY
    if text in ["y", "Y", "year", "Year"]:
        return PERIOD.YEARLY
    return PERIOD.DAILY


def _period_from_min(freq: str) -> tuple[PERIOD, str, int]:
    text = str(freq)
    if text in ["5", "5m", "5min", "five"]:
        return PERIOD.MIN_5, "5min", 48
    if text in ["1", "1m", "1min", "one"]:
        return PERIOD.MIN_1, "1min", 240
    if text in ["15", "15m", "15min", "fifteen"]:
        return PERIOD.MIN_15, "15min", 16
    if text in ["30", "30m", "30min", "half"]:
        return PERIOD.MIN_30, "30min", 8
    if text in ["60", "60m", "60min", "1h"]:
        return PERIOD.MIN_60, "60min", 4
    return PERIOD.MIN_1, "1min", 240


def _fetch_standard_kline(client, market: MARKET, code: str, period: PERIOD, lens: int, batch: int = 800) -> list[dict]:
    rows = []
    pages = int(lens / batch) + 1
    for i in range(pages):
        start = (int(lens / batch) - i) * batch
        part = client.stock_kline(market, str(code), period, start=start, count=batch)
        if part:
            rows.extend(part)
    return rows


def _fetch_index_kline(client, market: MARKET, code: str, period: PERIOD, lens: int, batch: int = 800) -> list[dict]:
    rows = []
    pages = int(lens / batch) + 1
    for i in range(pages):
        start = (int(lens / batch) - i) * batch
        part = client.index_kline(market, str(code), period, start=start, count=batch)
        if part:
            rows.extend(part)
    return rows


def _fetch_goods_kline(client, market: EX_MARKET, code: str, period: PERIOD, lens: int, batch: int = 700) -> list[dict]:
    rows = []
    pages = int(lens / batch) + 1
    for i in range(pages):
        start = (int(lens / batch) - i) * batch
        part = client.goods_kline(market, str(code), period, start=start, count=batch)
        if part:
            rows.extend(part)
    return rows


def _normalize_standard_day(rows: list[dict], code: str, start_date: str, end_date: str) -> pd.DataFrame | None:
    if not rows:
        return None
    data = pd.DataFrame(rows)
    if len(data) < 1:
        return None
    if "open" in data.columns:
        data = data[data["open"] != 0]
    if len(data) < 1:
        return None
    data["date"] = data["datetime"].apply(lambda x: str(x)[0:10])
    data["code"] = str(code)
    data["date_stamp"] = data["date"].apply(QA_util_date_stamp)
    if "vol" in data.columns:
        data["vol"] = pd.to_numeric(data["vol"], errors="coerce").fillna(0) / 100
    data = data.set_index("date", drop=False)
    drop_cols = [c for c in ["market", "name", "category", "vol_unit", "pre_close", "avg", "industry", "momentum", "float_shares", "turnover", "datetime"] if c in data.columns]
    data = data.drop(columns=drop_cols, errors="ignore")
    data = data.loc[str(start_date)[0:10]:str(end_date)[0:10]]
    return data if len(data) > 0 else None


def _normalize_standard_min(rows: list[dict], code: str, start: str, end: str, type_: str) -> pd.DataFrame | None:
    if not rows:
        return None
    data = pd.DataFrame(rows)
    if len(data) < 1:
        return None
    data["datetime"] = pd.to_datetime(data["datetime"], utc=False)
    data["code"] = str(code)
    data["date"] = data["datetime"].apply(lambda x: str(x)[0:10])
    data["date_stamp"] = data["datetime"].apply(QA_util_date_stamp)
    data["time_stamp"] = data["datetime"].apply(QA_util_time_stamp)
    data["type"] = type_
    drop_cols = [c for c in ["market", "name", "category", "vol_unit", "pre_close", "avg", "industry", "momentum", "float_shares", "turnover"] if c in data.columns]
    data = data.drop(columns=drop_cols, errors="ignore")
    data = data.set_index("datetime", drop=False)[start:end]
    data["datetime"] = data["datetime"].apply(str)
    return data if len(data) > 0 else None


def _normalize_security_bars(rows: list[dict], code: str, type_: str) -> pd.DataFrame | None:
    if not rows:
        return None
    if _is_day_frequency(type_):
        data = _normalize_standard_day(rows, str(code), "1990-01-01", str(date.today()))
        if data is None:
            return None
        return data
    _, normalized_type, _ = _period_from_min(type_)
    data = _normalize_standard_min(rows, str(code), "1990-01-01", str(datetime.datetime.now()), normalized_type)
    if data is None:
        return None
    return data


def _extract_handicap_value(handicap: dict, side: str, idx: int, field: str, default=0):
    try:
        values = handicap.get(side, [])
        if idx < len(values):
            return values[idx].get(field, default)
    except Exception:
        pass
    return default


def _normalize_quote_result(rows: list[dict], code_selector, is_bond: bool = False) -> pd.DataFrame:
    if not rows:
        return pd.DataFrame()
    now = datetime.datetime.now()
    norm = []
    for row in rows:
        handicap = row.get("handicap", {}) or {}
        item = {
            "datetime": now,
            "servertime": row.get("server_time"),
            "active1": row.get("active", 0),
            "active2": row.get("rise_speed", 0),
            "last_close": row.get("pre_close", 0),
            "code": str(row.get("code", "")),
            "open": row.get("open", 0),
            "high": row.get("high", 0),
            "low": row.get("low", 0),
            "price": row.get("close", row.get("price", 0)),
            "cur_vol": row.get("cur_vol", 0),
            "s_vol": row.get("in_vol", 0),
            "b_vol": row.get("out_vol", 0),
            "vol": row.get("vol", 0),
        }
        for i in range(5):
            item[f"bid{i+1}"] = _extract_handicap_value(handicap, "bid", i, "price", 0)
            item[f"bid_vol{i+1}"] = _extract_handicap_value(handicap, "bid", i, "vol", 0)
            item[f"ask{i+1}"] = _extract_handicap_value(handicap, "ask", i, "price", 0)
            item[f"ask_vol{i+1}"] = _extract_handicap_value(handicap, "ask", i, "vol", 0)
        norm.append(item)
    data = pd.DataFrame(norm)
    return data.set_index(["datetime", "code"])


def _classify_sz(code: str) -> str:
    code = str(code)
    if code[0:2] in ["00", "30", "02"]:
        return "stock_cn"
    if code[0:2] in ["39"]:
        return "index_cn"
    if code[0:2] in ["15", "16"]:
        return "etf_cn"
    if code[0:3] in ["101", "104", "105", "106", "107", "108", "109", "111", "112", "114", "115", "116", "117", "118", "119", "123", "127", "128", "131", "139"]:
        return "bond_cn"
    if code[0:2] in ["20"]:
        return "stockB_cn"
    return "undefined"


def _classify_sh(code: str) -> str:
    code = str(code)
    if code[0] == "6":
        return "stock_cn"
    if code[0:3] in ["000", "880"]:
        return "index_cn"
    if code[0:2] in ["51", "58"]:
        return "etf_cn"
    if code[0:3] in ["102", "110", "113", "120", "122", "124", "130", "132", "133", "134", "135", "136", "140", "141", "143", "144", "147", "148"]:
        return "bond_cn"
    return "undefined"


def _fetch_stock_list_all() -> pd.DataFrame:
    _ensure_opentdx()
    frames = []
    client = QuotationClient(auto_retry=True, raise_exception=True)
    connected = client.connect()
    if connected is None:
        raise RuntimeError("opentdx quotation client connect failed")
    client.login()
    try:
        for market, sse in [(MARKET.SZ, "sz"), (MARKET.SH, "sh")]:
            total = client.get_count(market) or 0
            for start in range(0, total + 1000, 1000):
                part = client.get_list(market, start=start, count=1000)
                if not part:
                    continue
                df = pd.DataFrame(part)
                if "vol" in df.columns and "volunit" not in df.columns:
                    df = df.rename(columns={"vol": "volunit"})
                df["sse"] = sse
                frames.append(df)
    finally:
        client.disconnect()
    if not frames:
        return pd.DataFrame()
    data = pd.concat(frames, axis=0, sort=False)
    keep = [c for c in ["code", "volunit", "decimal_point", "name", "pre_close", "sse"] if c in data.columns]
    data = data.loc[:, keep].drop_duplicates()
    data = data.set_index(["code", "sse"], drop=False)
    if "name" in data.columns:
        data["name"] = data["name"].apply(lambda x: str(x)[0:6])
    return data


def _ensure_extensionmarket_list() -> pd.DataFrame:
    global extension_market_list
    if extension_market_list is not None:
        return extension_market_list
    _ensure_opentdx()
    rows = []
    with TdxClient() as client:
        total = client.goods_count() or 0
        page = 500
        if total > 0:
            for start in range(0, total, page):
                current = min(page, total - start)
                part = client.goods_list(start=start, count=current)
                if not part:
                    continue
                rows.extend(part)
                if len(part) < current:
                    break
        if not rows:
            for probe in [100, 50, 10, 5]:
                part = client.goods_list(start=0, count=probe)
                if part:
                    rows.extend(part)
                    break
    extension_market_list = pd.DataFrame(rows)
    if len(extension_market_list) > 0 and "code" in extension_market_list.columns:
        extension_market_list = extension_market_list.drop_duplicates(subset=["code"]).set_index("code", drop=False)
    return extension_market_list


def _ensure_goods_varieties() -> pd.DataFrame:
    global goods_varieties_list
    if goods_varieties_list is not None:
        return goods_varieties_list
    _ensure_opentdx()
    markets = [
        EX_MARKET.ZZ_FUTURES.value,
        EX_MARKET.DL_FUTURES.value,
        EX_MARKET.SH_FUTURES.value,
        EX_MARKET.CFFEX_FUTURES.value,
        EX_MARKET.MAIN_FUTURES_CONTRACT.value,
        EX_MARKET.GZ_FUTURES.value,
    ]
    rows = []
    with TdxClient() as client:
        for market in markets:
            try:
                part = client.goods_varieties(market, start=0, count=600)
            except Exception:
                part = []
            if not part:
                continue
            for item in part:
                row = dict(item)
                row["market_id"] = market
                rows.append(row)
    goods_varieties_list = pd.DataFrame(rows)
    return goods_varieties_list


def _code_product_prefix(code: str) -> str:
    return "".join(ch for ch in str(code).upper() if ch.isalpha())


def _to_ex_market(value):
    try:
        value = int(value)
    except Exception:
        return None
    if value in _VALID_EX_MARKET_VALUES:
        return EX_MARKET(value)
    return None


def _resolve_extension_market(row):
    direct_market = _to_ex_market(row.get("market"))
    if direct_market is not None:
        return direct_market
    category_market = _to_ex_market(row.get("category"))
    if category_market is not None:
        return category_market
    return None


def _get_goods_market(code: str) -> EX_MARKET:
    data = _ensure_extensionmarket_list()
    if data is None or len(data) < 1:
        data = pd.DataFrame()
    row = data.query('code=="{}"'.format(code))
    if len(row) > 0:
        market = _resolve_extension_market(row.iloc[0])
        if market is not None:
            return market

    prefix = _code_product_prefix(code)
    varieties = _ensure_goods_varieties()
    if len(varieties) > 0:
        code_upper = str(code).upper()
        for _, item in varieties.iterrows():
            codes = item.get("code", [])
            if not isinstance(codes, list):
                continue
            for sample in codes:
                if str(sample).upper() == code_upper:
                    market = _to_ex_market(item.get("market_id"))
                    if market is not None:
                        return market
        if prefix:
            for _, item in varieties.iterrows():
                codes = item.get("code", [])
                if not isinstance(codes, list):
                    continue
                for sample in codes:
                    if prefix == _code_product_prefix(sample):
                        market = _to_ex_market(item.get("market_id"))
                        if market is not None:
                            return market
    raise KeyError(f"code not found in extension market list: {code}")


def _filter_extension(query: str) -> pd.DataFrame:
    data = _ensure_extensionmarket_list()
    if data is None or len(data) < 1:
        return pd.DataFrame()
    return data.query(query)


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_stock_day(code, start_date, end_date, if_fq="00", frequence="day", ip=None, port=None):
    if if_fq not in ["00", "bfq"]:
        print("CURRENTLY NOT SUPPORT REALTIME FUQUAN")
        return None
    try:
        _ensure_opentdx()
        period = _period_from_day(frequence)
        start_date = str(start_date)[0:10]
        lens = QA_util_get_trade_gap(start_date, datetime.date.today())
        with TdxClient() as client:
            rows = _fetch_standard_kline(client, _stock_market(code), str(code), period, lens)
        return _normalize_standard_day(rows, str(code), start_date, end_date)
    except Exception as e:
        print(e)
        return None


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_stock_min(code, start, end, frequence="1min", ip=None, port=None):
    try:
        _ensure_opentdx()
        start_date = str(start)[0:10]
        period, type_, multiplier = _period_from_min(frequence)
        lens = QA_util_get_trade_gap(start_date, datetime.date.today()) * multiplier
        lens = min(lens, 20800)
        with TdxClient() as client:
            rows = _fetch_standard_kline(client, _stock_market(code), str(code), period, lens)
        return _normalize_standard_min(rows, str(code), start, end, type_)
    except Exception as e:
        print(e)
        return None


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_stock_realtime(code=["000001", "000002"], ip=None, port=None):
    try:
        _ensure_opentdx()
        code = [code] if isinstance(code, str) else code
        targets = [(_stock_market(item), str(item)) for item in code]
        with TdxClient() as client:
            rows = client.stock_quotes_detail(targets)
        return _normalize_quote_result(rows, _stock_market)
    except Exception as e:
        print(e)
        return pd.DataFrame()


def QA_fetch_get_trade_date(end, exchange):
    if str(exchange).upper() not in ["SSE", "SZSE", "SH", "SZ"]:
        return []
    real_start, real_end = QA_util_get_real_datelist("1990-01-01", end)
    if real_start is None:
        return []
    try:
        start_idx = trade_date_sse.index(real_start)
        end_idx = trade_date_sse.index(real_end)
    except ValueError:
        return []
    return trade_date_sse[start_idx:end_idx + 1]


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_security_bars(code, _type, lens, ip=None, port=None):
    try:
        _ensure_opentdx()
        lens = int(lens)
        if lens < 1:
            return None
        market = _stock_market(code)
        period = _period_from_day(_type) if _is_day_frequency(_type) else _period_from_min(_type)[0]
        with TdxClient() as client:
            rows = _fetch_standard_kline(client, market, str(code), period, lens, batch=min(max(lens, 1), 800))
        data = _normalize_security_bars(rows, str(code), _type)
        return data.tail(lens) if data is not None else None
    except Exception as e:
        print(e)
        return None


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_index_realtime(code=["000001"], ip=None, port=None):
    try:
        _ensure_opentdx()
        code = [code] if isinstance(code, str) else code
        targets = [(_index_market(item), str(item)) for item in code]
        with TdxClient() as client:
            rows = client.stock_quotes_detail(targets)
        return _normalize_quote_result(rows, _index_market)
    except Exception as e:
        print(e)
        return pd.DataFrame()


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_bond_realtime(code=["010107"], ip=None, port=None):
    try:
        _ensure_opentdx()
        code = [code] if isinstance(code, str) else code
        targets = [(_bond_market(item), str(item)) for item in code]
        with TdxClient() as client:
            rows = client.stock_quotes_detail(targets)
        return _normalize_quote_result(rows, _bond_market, is_bond=True)
    except Exception as e:
        print(e)
        return pd.DataFrame()


def QA_fetch_get_stock_list(type_="stock", ip=None, port=None):
    try:
        data = _fetch_stock_list_all()
        if len(data) < 1:
            return data
        sz = data.query('sse=="sz"').assign(sec=lambda x: x.code.apply(_classify_sz))
        sh = data.query('sse=="sh"').assign(sec=lambda x: x.code.apply(_classify_sh))
        merged = pd.concat([sz, sh], sort=False)
        if type_ in ["stock", "gp"]:
            return merged.query('sec=="stock_cn"').sort_index()
        if type_ in ["index", "zs"]:
            return merged.query('sec=="index_cn"').sort_index()
        if type_ in ["etf", "ETF"]:
            return merged.query('sec=="etf_cn"').sort_index()
        return merged.sort_index()
    except Exception as e:
        print(e)
        return pd.DataFrame()


def QA_fetch_get_index_list(ip=None, port=None):
    data = QA_fetch_get_stock_list(type_="index", ip=ip, port=port)
    return data if isinstance(data, pd.DataFrame) else pd.DataFrame()


def QA_fetch_get_bond_list(ip=None, port=None):
    try:
        data = _fetch_stock_list_all()
        if len(data) < 1:
            return data
        sz = data.query('sse=="sz"').assign(sec=lambda x: x.code.apply(_classify_sz))
        sh = data.query('sse=="sh"').assign(sec=lambda x: x.code.apply(_classify_sh))
        return pd.concat([sz, sh], sort=False).query('sec=="bond_cn"').sort_index()
    except Exception as e:
        print(e)
        return pd.DataFrame()


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_bond_day(code, start_date, end_date, frequence="day", ip=None, port=None):
    try:
        _ensure_opentdx()
        period = _period_from_day(frequence)
        start_date = str(start_date)[0:10]
        lens = QA_util_get_trade_gap(start_date, datetime.date.today())
        with TdxClient() as client:
            rows = _fetch_standard_kline(client, _bond_market(code), str(code), period, lens)
        return _normalize_standard_day(rows, str(code), start_date, end_date)
    except Exception as e:
        print(e)
        return None


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_bond_min(code, start, end, frequence="1min", ip=None, port=None):
    try:
        _ensure_opentdx()
        start_date = str(start)[0:10]
        period, type_, multiplier = _period_from_min(frequence)
        lens = QA_util_get_trade_gap(start_date, datetime.date.today()) * multiplier
        lens = min(lens, 20800)
        with TdxClient() as client:
            rows = _fetch_standard_kline(client, _bond_market(code), str(code), period, lens)
        return _normalize_standard_min(rows, str(code), start, end, type_)
    except Exception as e:
        print(e)
        return None


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_index_day(code, start_date, end_date, frequence="day", ip=None, port=None):
    try:
        _ensure_opentdx()
        period = _period_from_day(frequence)
        start_date = str(start_date)[0:10]
        lens = QA_util_get_trade_gap(start_date, datetime.date.today())
        use_stock_channel = _index_uses_stock_channel(code)
        market = _stock_market(code) if use_stock_channel else _index_market(code)
        with TdxClient() as client:
            rows = (
                _fetch_standard_kline(client, market, str(code), period, lens)
                if use_stock_channel else
                _fetch_index_kline(client, market, str(code), period, lens)
            )
        data = _normalize_standard_day(rows, str(code), start_date, end_date)
        if use_stock_channel and data is not None:
            data = data.drop(columns=["up_count", "down_count"], errors="ignore")
        return data
    except Exception as e:
        print(e)
        return None


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_index_min(code, start, end, frequence="1min", ip=None, port=None):
    try:
        _ensure_opentdx()
        start_date = str(start)[0:10]
        period, type_, multiplier = _period_from_min(frequence)
        lens = QA_util_get_trade_gap(start_date, datetime.date.today()) * multiplier
        lens = min(lens, 20800)
        use_stock_channel = _index_uses_stock_channel(code)
        market = _stock_market(code) if use_stock_channel else _index_market(code)
        with TdxClient() as client:
            rows = (
                _fetch_standard_kline(client, market, str(code), period, lens)
                if use_stock_channel else
                _fetch_index_kline(client, market, str(code), period, lens)
            )
        data = _normalize_standard_min(rows, str(code), start, end, type_)
        if use_stock_channel and data is not None:
            data = data.drop(columns=["up_count", "down_count"], errors="ignore")
        return data
    except Exception as e:
        print(e)
        return None


def _normalize_transaction_rows(rows: list[dict], code: str, day_text: str, type_: str = "tick") -> pd.DataFrame | None:
    if not rows:
        return None
    data = pd.DataFrame(rows)
    if len(data) < 1:
        return None
    data["date"] = day_text
    data["datetime"] = pd.to_datetime(data["time"].apply(lambda x: f"{day_text} {x}"), utc=False)
    data["code"] = str(code)
    data["date_stamp"] = data["datetime"].apply(QA_util_date_stamp)
    data["time_stamp"] = data["datetime"].apply(QA_util_time_stamp)
    data["type"] = type_
    data["order"] = range(len(data.index))
    data = data.set_index("datetime", drop=False)
    data["datetime"] = data["datetime"].apply(lambda x: str(x)[0:19])
    return data


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_stock_transaction(code, start, end, retry=2, ip=None, port=None):
    try:
        _ensure_opentdx()
        real_start, real_end = QA_util_get_real_datelist(start, end)
        if real_start is None:
            return None
        market = _stock_market(code)
        data = pd.DataFrame()
        with TdxClient() as client:
            for index_ in range(trade_date_sse.index(real_start), trade_date_sse.index(real_end) + 1):
                day_text = trade_date_sse[index_]
                day = datetime.datetime.strptime(day_text, "%Y-%m-%d").date()
                rows = client.stock_transaction(market, str(code), day)
                part = _normalize_transaction_rows(rows, str(code), day_text)
                if part is not None:
                    data = pd.concat([data, part], ignore_index=True)
        return data if len(data) > 0 else None
    except Exception as e:
        print(e)
        return None


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_index_transaction(code, start, end, retry=2, ip=None, port=None):
    try:
        _ensure_opentdx()
        real_start, real_end = QA_util_get_real_datelist(start, end)
        if real_start is None:
            return None
        market = _index_market(code)
        data = pd.DataFrame()
        with TdxClient() as client:
            for index_ in range(trade_date_sse.index(real_start), trade_date_sse.index(real_end) + 1):
                day_text = trade_date_sse[index_]
                day = datetime.datetime.strptime(day_text, "%Y-%m-%d").date()
                rows = client.stock_transaction(market, str(code), day)
                part = _normalize_transaction_rows(rows, str(code), day_text)
                if part is not None:
                    data = pd.concat([data, part], ignore_index=True)
        return data if len(data) > 0 else None
    except Exception as e:
        print(e)
        return None


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_stock_transaction_realtime(code, ip=None, port=None):
    try:
        _ensure_opentdx()
        day = datetime.date.today()
        with TdxClient() as client:
            rows = client.stock_transaction(_stock_market(code), str(code), None)
        return _normalize_transaction_rows(rows, str(code), str(day))
    except Exception as e:
        print(e)
        return None


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_stock_xdxr(code, ip=None, port=None):
    try:
        _ensure_opentdx()
        with TdxClient() as client:
            rows = client.stock_xdxr(_stock_market(code), str(code))
        if not rows:
            return None
        data = pd.DataFrame(rows)
        if len(data) < 1:
            return None
        data["category"] = data["name"].apply(lambda x: _XDXR_CATEGORY_NAME.get(str(x)))
        data["category_meaning"] = data["name"].apply(lambda x: str(x))
        data["code"] = str(code)
        data = data.rename(columns={
            "panhouliutong": "liquidity_after",
            "panqianliutong": "liquidity_before",
            "houzongguben": "shares_after",
            "qianzongguben": "shares_before",
        })
        keep = [
            "category", "name", "fenhong", "peigujia", "songzhuangu", "peigu",
            "suogu", "liquidity_after", "liquidity_before", "shares_after",
            "shares_before", "fenshu", "xingquanjia", "date",
            "category_meaning", "code"
        ]
        for col in keep:
            if col not in data.columns:
                data[col] = None
        data = data[keep].set_index("date", drop=False)
        data["date"] = data["date"].apply(lambda x: str(x)[0:10])
        return data
    except Exception as e:
        print(e)
        return None


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_stock_info(code, ip=None, port=None):
    try:
        _ensure_opentdx()
        with TdxClient() as client:
            row = client.stock_finance(_stock_market(code), str(code))
        return pd.DataFrame([row]) if row else pd.DataFrame()
    except Exception as e:
        print(e)
        return pd.DataFrame()


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_stock_block(ip=None, port=None):
    try:
        _ensure_opentdx()
        frames = []
        with TdxClient() as client:
            for block_type, type_label in _STANDARD_BLOCK_TYPES:
                rows = client.stock_block(block_type)
                if not rows:
                    continue
                df = pd.DataFrame(rows)
                if len(df) < 1:
                    continue
                keep = [c for c in ["blockname", "code"] if c in df.columns]
                if len(keep) < 2:
                    continue
                df = df[keep]
                df["type"] = type_label
                df["source"] = "opentdx"
                frames.append(df)
        if not frames:
            return pd.DataFrame()
        data = pd.concat(frames, ignore_index=True, sort=False)
        return data.set_index("code", drop=False).drop_duplicates()
    except Exception as e:
        QA_util_log_info(f"Wrong with fetch block {e}")
        return pd.DataFrame()


def QA_fetch_get_extensionmarket_list(ip=None, port=None, batch=2000):
    try:
        return _ensure_extensionmarket_list()
    except Exception as e:
        print(e)
        return pd.DataFrame()


def QA_fetch_get_future_list(ip=None, port=None):
    data = _ensure_extensionmarket_list().reset_index(drop=True).copy()
    if len(data) > 0:
        data["resolved_market"] = data.apply(_resolve_extension_market, axis=1)
        data = data[data["resolved_market"].notna()].copy()
        if len(data) > 0:
            data["market"] = data["resolved_market"].apply(lambda x: x.value)
            data = data[data["market"].isin([
                EX_MARKET.ZZ_FUTURES.value,
                EX_MARKET.DL_FUTURES.value,
                EX_MARKET.SH_FUTURES.value,
                EX_MARKET.CFFEX_FUTURES.value,
                EX_MARKET.MAIN_FUTURES_CONTRACT.value,
                EX_MARKET.GZ_FUTURES.value,
                EX_MARKET.FUTURES_INDEX.value,
            ])]
            keep = [c for c in ["market", "category", "code", "desc", "name"] if c in data.columns]
            data = data[keep].drop_duplicates(subset=["market", "code"]).sort_values(["market", "code"]).reset_index(drop=True)
            if len(data) > 0:
                return data

    varieties = _ensure_goods_varieties()
    if len(varieties) < 1:
        return pd.DataFrame(columns=["market", "category", "code", "desc", "name"])
    rows = []
    for _, item in varieties.iterrows():
        rows.append({
            "market": item.get("market_id"),
            "category": item.get("category"),
            "code": "",
            "desc": item.get("code"),
            "name": item.get("name"),
        })
    data = pd.DataFrame(rows)
    return data.drop_duplicates(subset=["market", "name"]).sort_values(["market", "name"]).reset_index(drop=True)


def QA_fetch_get_globalindex_list(ip=None, port=None):
    return _filter_extension("market==12 or market==37")


def QA_fetch_get_goods_list(ip=None, port=None):
    return _filter_extension("market==50 or market==76 or market==46")


def QA_fetch_get_globalfuture_list(ip=None, port=None):
    return _filter_extension("market==14 or market==15 or market==16 or market==17 or market==18 or market==19 or market==20 or market==77 or market==39")


def QA_fetch_get_hkstock_list(ip=None, port=None):
    return _filter_extension("market==31 or market==48")


def QA_fetch_get_hkindex_list(ip=None, port=None):
    return _filter_extension("market==27")


def QA_fetch_get_hkfund_list(ip=None, port=None):
    return _filter_extension("market==49")


def QA_fetch_get_usstock_list(ip=None, port=None):
    return _filter_extension("market==74 or market==40 or market==41")


def QA_fetch_get_macroindex_list(ip=None, port=None):
    return _filter_extension("market==38")


def QA_fetch_get_option_list(ip=None, port=None):
    return _filter_extension("category==12 and market!=1")


def QA_fetch_get_exchangerate_list(ip=None, port=None):
    return _filter_extension("(market==10 or market==11) and category==4")


def _normalize_goods_day(rows: list[dict], code: str, start_date: str, end_date: str) -> pd.DataFrame | None:
    if not rows:
        return None
    data = pd.DataFrame(rows)
    if len(data) < 1:
        return None
    if "date_time" in data.columns and "datetime" not in data.columns:
        data = data.rename(columns={"date_time": "datetime"})
    data["date"] = data["datetime"].apply(lambda x: str(x)[0:10])
    data["code"] = str(code)
    data["date_stamp"] = data["date"].apply(QA_util_date_stamp)
    if "position" not in data.columns and "amount" in data.columns:
        data["position"] = data["amount"]
    if "price" not in data.columns:
        if "settlementprice" in data.columns:
            data["price"] = data["settlementprice"]
        elif "close" in data.columns:
            data["price"] = data["close"]
    if "trade" not in data.columns and "vol" in data.columns:
        data["trade"] = data["vol"]
    data = data.set_index("date", drop=False)
    data = data.loc[str(start_date)[0:10]:str(end_date)[0:10]]
    drop_cols = [c for c in ["market", "name", "category", "datetime"] if c in data.columns]
    data = data.drop(columns=drop_cols, errors="ignore")
    return data if len(data) > 0 else None


def _normalize_goods_min(rows: list[dict], code: str, start: str, end: str, type_: str) -> pd.DataFrame | None:
    if not rows:
        return None
    data = pd.DataFrame(rows)
    if len(data) < 1:
        return None
    if "date_time" in data.columns and "datetime" not in data.columns:
        data = data.rename(columns={"date_time": "datetime"})
    data["datetime"] = pd.to_datetime(data["datetime"], utc=False)
    data["tradetime"] = data["datetime"].apply(str)
    data["code"] = str(code)
    data["date"] = data["datetime"].apply(lambda x: str(x)[0:10])
    data["date_stamp"] = data["datetime"].apply(QA_util_date_stamp)
    data["time_stamp"] = data["datetime"].apply(QA_util_time_stamp)
    data["type"] = type_
    if "position" not in data.columns and "amount" in data.columns:
        data["position"] = data["amount"]
    if "price" not in data.columns:
        if "settlementprice" in data.columns:
            data["price"] = data["settlementprice"]
        elif "close" in data.columns:
            data["price"] = data["close"]
    if "trade" not in data.columns and "vol" in data.columns:
        data["trade"] = data["vol"]
    data = data.set_index("datetime", drop=False)[start:end].sort_index()
    data["datetime"] = data["datetime"].apply(str)
    return data if len(data) > 0 else None


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_future_day(code, start_date, end_date, frequence="day", ip=None, port=None):
    try:
        _ensure_opentdx()
        start_date = str(start_date)[0:10]
        market = _get_goods_market(code)
        with TdxClient() as client:
            if str(frequence).lower() in ["day", "d", "daily"]:
                rows = client.goods_kline_by_date(market, str(code), start_date, end_date)
            else:
                lens = QA_util_get_trade_gap(start_date, datetime.date.today())
                period = _period_from_day(frequence)
                rows = _fetch_goods_kline(client, market, str(code), period, lens)
        return _normalize_goods_day(rows, str(code), start_date, end_date)
    except Exception as e:
        print("code is ", code)
        print(e)
        return None


@retry(stop_max_attempt_number=3, wait_random_min=50, wait_random_max=100)
def QA_fetch_get_future_min(code, start, end, frequence="1min", ip=None, port=None):
    try:
        _ensure_opentdx()
        start_date = str(start)[0:10]
        period, type_, multiplier = _period_from_min(frequence)
        lens = QA_util_get_trade_gap(start_date, datetime.date.today()) * multiplier * 2.5
        lens = min(int(lens), 20800)
        market = _get_goods_market(code)
        with TdxClient() as client:
            rows = _fetch_goods_kline(client, market, str(code), period, lens)
        return _normalize_goods_min(rows, str(code), start, end, type_)
    except Exception as e:
        print(e)
        return None


def _normalize_goods_transaction_rows(rows: list[dict], code: str, day_text: str) -> pd.DataFrame | None:
    if not rows:
        return None
    data = pd.DataFrame(rows)
    if len(data) < 1:
        return None
    data["date"] = str(day_text)
    data["datetime"] = pd.to_datetime(data["time"].apply(lambda x: f"{day_text} {x}"), utc=False)
    data["code"] = str(code)
    if "volume" not in data.columns and "vol" in data.columns:
        data["volume"] = data["vol"]
    if "price" in data.columns:
        # Legacy pytdx extended-market tick prices are stored as integer
        # thousandths; existing strategy code divides by 1000 after fetch.
        data["price"] = pd.to_numeric(data["price"], errors="coerce") * 1000
    data["order"] = range(len(data.index))
    data = data.set_index("datetime", drop=False)
    data["datetime"] = data["datetime"].apply(lambda x: str(x)[0:19])
    return data


def _normalize_goods_quote_rows(rows: list[dict]) -> pd.DataFrame:
    if not rows:
        return pd.DataFrame()
    data = pd.DataFrame(rows)
    rename_map = {
        "close": "price",
        "pre_close": "last_close",
        "curr_vol": "cur_vol",
        "in_vol": "s_vol",
        "out_vol": "b_vol",
    }
    data = data.rename(columns={old: new for old, new in rename_map.items() if old in data.columns})
    data["datetime"] = datetime.datetime.now()
    return data.set_index(["datetime", "code"])


def QA_fetch_get_future_transaction(code, start, end, retry=4, ip=None, port=None):
    try:
        _ensure_opentdx()
        real_start, real_end = QA_util_get_real_datelist(start, end)
        if real_start is None:
            return None
        market = _get_goods_market(code)
        data = pd.DataFrame()
        with TdxClient() as client:
            for index_ in range(trade_date_sse.index(real_start), trade_date_sse.index(real_end) + 1):
                day_text = trade_date_sse[index_]
                query_day = datetime.datetime.strptime(day_text, "%Y-%m-%d").date()
                rows = client.goods_history_transaction(market, str(code), query_day)
                part = _normalize_goods_transaction_rows(rows, str(code), day_text)
                if part is not None:
                    data = pd.concat([data, part], ignore_index=True)
        return data if len(data) > 0 else None
    except Exception as e:
        print(e)
        return None


def QA_fetch_get_future_transaction_realtime(code, ip=None, port=None):
    try:
        _ensure_opentdx()
        market = _get_goods_market(code)
        today = datetime.date.today()
        with TdxClient() as client:
            rows = client.goods_history_transaction(market, str(code), today)
        return _normalize_goods_transaction_rows(rows, str(code), str(today))
    except Exception as e:
        print(e)
        return None


def QA_fetch_get_future_realtime(code, ip=None, port=None):
    try:
        _ensure_opentdx()
        market = _get_goods_market(code)
        with TdxClient() as client:
            rows = client.goods_quotes(market, str(code))
        return _normalize_goods_quote_rows(rows)
    except Exception as e:
        print(e)
        return pd.DataFrame()


def QA_fetch_get_stock_board_list(type_="all", ip=None, port=None):
    try:
        _ensure_opentdx()
        type_map = {
            "all": BOARD_TYPE.ALL,
            "hy": BOARD_TYPE.HY,
            "hy2": BOARD_TYPE.HY2,
            "gn": BOARD_TYPE.GN,
            "fg": BOARD_TYPE.FG,
            "dq": BOARD_TYPE.DQ,
            "hk": EX_BOARD_TYPE.HK_ALL,
            "us": EX_BOARD_TYPE.US_ALL,
        }
        board_type = type_map.get(str(type_).lower(), BOARD_TYPE.ALL)
        with TdxClient() as client:
            rows = client.stock_board_list(board_type)
        data = pd.DataFrame(rows)
        return data.set_index("board_symbol", drop=False) if len(data) > 0 else pd.DataFrame()
    except Exception as e:
        print(e)
        return pd.DataFrame()


def QA_fetch_get_stock_board_members(board_symbol="881001", ip=None, port=None):
    try:
        _ensure_opentdx()
        with TdxClient() as client:
            rows = client.stock_board_members(board_symbol)
        return pd.DataFrame(rows)
    except Exception as e:
        print(e)
        return pd.DataFrame()


def QA_fetch_get_stock_capital_flow(code, ip=None, port=None):
    try:
        _ensure_opentdx()
        with TdxClient() as client:
            return client.stock_capital_flow(_stock_market(code), str(code))
    except Exception as e:
        print(e)
        return pd.DataFrame()


def QA_fetch_get_goods_kline_by_date(market: EX_MARKET, code: str, start_date, end_date, ip=None, port=None):
    try:
        _ensure_opentdx()
        with TdxClient() as client:
            rows = client.goods_kline_by_date(market, str(code), start_date, end_date)
        if not rows:
            return None
        data = pd.DataFrame(rows)
        data["date"] = data["datetime"].apply(lambda x: str(x)[0:10])
        data["date_stamp"] = data["date"].apply(QA_util_date_stamp)
        data["code"] = str(code)
        return data.set_index("date", drop=False)
    except Exception as e:
        print(e)
        return None


def QA_fetch_get_company_info_category(code, ip=None, port=None):
    try:
        _ensure_opentdx()
        with TdxClient() as client:
            rows = client.stock_company_info_category(_stock_market(code), str(code))
        return pd.DataFrame(rows)
    except Exception as e:
        print(e)
        return pd.DataFrame()


def QA_fetch_get_company_info_content(code, filename, start, length, ip=None, port=None):
    try:
        _ensure_opentdx()
        with TdxClient() as client:
            return client.stock_company_info_content(_stock_market(code), str(code), filename, start, length)
    except Exception as e:
        print(e)
        return {}


QA_fetch_get_option_realtime = QA_fetch_get_future_realtime
QA_fetch_get_option_transaction_realtime = QA_fetch_get_future_transaction_realtime

QA_fetch_get_hkstock_day = QA_fetch_get_future_day
QA_fetch_get_hkstock_min = QA_fetch_get_future_min
QA_fetch_get_hkfund_day = QA_fetch_get_future_day
QA_fetch_get_hkfund_min = QA_fetch_get_future_min
QA_fetch_get_hkindex_day = QA_fetch_get_future_day
QA_fetch_get_hkindex_min = QA_fetch_get_future_min
QA_fetch_get_usstock_day = QA_fetch_get_future_day
QA_fetch_get_usstock_min = QA_fetch_get_future_min
QA_fetch_get_option_day = QA_fetch_get_future_day
QA_fetch_get_option_min = QA_fetch_get_future_min
QA_fetch_get_globalfuture_day = QA_fetch_get_future_day
QA_fetch_get_globalfuture_min = QA_fetch_get_future_min
QA_fetch_get_exchangerate_day = QA_fetch_get_future_day
QA_fetch_get_exchangerate_min = QA_fetch_get_future_min
QA_fetch_get_macroindex_day = QA_fetch_get_future_day
QA_fetch_get_macroindex_min = QA_fetch_get_future_min
QA_fetch_get_globalindex_day = QA_fetch_get_future_day
QA_fetch_get_globalindex_min = QA_fetch_get_future_min
