# coding:utf-8

import datetime

import pymongo

from QUANTAXIS.QAData.data_fq import _QA_data_stock_to_fq
from QUANTAXIS.QAFetch import QA_fetch_get_stock_block
from QUANTAXIS.QAFetch.QAOpentdx import (
    QA_fetch_get_bond_day,
    QA_fetch_get_bond_list,
    QA_fetch_get_bond_min,
    QA_fetch_get_future_day,
    QA_fetch_get_future_list,
    QA_fetch_get_future_min,
    QA_fetch_get_index_day,
    QA_fetch_get_index_list,
    QA_fetch_get_index_min,
    QA_fetch_get_stock_day,
    QA_fetch_get_stock_info,
    QA_fetch_get_stock_list,
    QA_fetch_get_stock_min,
    QA_fetch_get_stock_transaction,
    QA_fetch_get_stock_xdxr,
    QA_fetch_get_index_transaction,
)
from QUANTAXIS.QAFetch.QAQuery import QA_fetch_stock_day
from QUANTAXIS.QAUtil import (
    DATABASE,
    QA_util_get_next_day,
    QA_util_get_real_date,
    QA_util_log_info,
    QA_util_to_json_from_pandas,
    trade_date_sse,
)


_MIN_TYPES = ["1min", "5min", "15min", "30min", "60min"]


def now_time():
    return (
        str(
            QA_util_get_real_date(
                str(datetime.date.today() - datetime.timedelta(days=1)),
                trade_date_sse,
                -1,
            )
        )
        + " 17:00:00"
        if datetime.datetime.now().hour < 15
        else str(
            QA_util_get_real_date(
                str(datetime.date.today()),
                trade_date_sse,
                -1,
            )
        )
        + " 15:00:00"
    )


def _codes_from_frame(frame):
    if frame is None or len(frame) < 1:
        return []
    if "code" in frame.columns:
        return frame["code"].astype(str).unique().tolist()
    if getattr(frame.index, "nlevels", 1) > 1:
        return frame.index.get_level_values(0).astype(str).unique().tolist()
    return frame.index.astype(str).unique().tolist()


def _get_stock_list_with_fallback(client=DATABASE):
    try:
        codes = _codes_from_frame(QA_fetch_get_stock_list())
        if codes:
            return codes
    except Exception:
        pass

    from QUANTAXIS.QAFetch.QAQuery import QA_fetch_stock_list

    frame = QA_fetch_stock_list(collections=client.stock_list)
    if frame is None or len(frame) == 0:
        raise ValueError(
            "无法获取股票列表。opentdx 返回空结果或连接失败，且 MongoDB stock_list 为空。"
            "请先准备可用股票列表。"
        )
    return _codes_from_frame(frame)


def _get_index_list_with_fallback(client=DATABASE):
    codes = _codes_from_frame(QA_fetch_get_index_list())
    if codes:
        return codes

    from QUANTAXIS.QAFetch.QAQuery import QA_fetch_index_list

    frame = QA_fetch_index_list(collections=client.index_list)
    if frame is None or len(frame) == 0:
        raise ValueError(
            "无法获取指数列表。opentdx 返回空结果，且 MongoDB index_list 为空。"
            "请先准备可用指数列表。"
        )
    return _codes_from_frame(frame)


def _create_day_index(coll):
    coll.create_index(
        [("code", pymongo.ASCENDING), ("date_stamp", pymongo.ASCENDING)]
    )


def _create_min_index(coll):
    coll.create_index(
        [
            ("code", pymongo.ASCENDING),
            ("time_stamp", pymongo.ASCENDING),
            ("date_stamp", pymongo.ASCENDING),
        ]
    )


def _insert_frame(coll, frame, skip_first=False):
    if frame is None or len(frame) < 1:
        return
    payload = QA_util_to_json_from_pandas(frame[1::] if skip_first else frame)
    if payload:
        coll.insert_many(payload)


def _log_progress(item, total, ui_log=None, ui_progress=None):
    QA_util_log_info("The {} of Total {}".format(item, total), ui_log=ui_log)
    if total:
        progress = int(float(item / total * 10000.0))
        QA_util_log_info(
            "DOWNLOAD PROGRESS {} ".format(
                str(float(item / total * 100))[0:4] + "%"
            ),
            ui_log=ui_log,
            ui_progress=ui_progress,
            ui_progress_int_value=progress,
        )


def _save_day_code(
    code,
    coll,
    fetcher,
    start_default,
    code_slice,
    job_name,
    ui_log=None,
):
    ref = list(coll.find({"code": str(code)[:code_slice]}))
    end_date = str(now_time())[:10]
    start_date = max(r["date"] for r in ref) if ref else start_default
    QA_util_log_info(
        "{} \n Trying updating {} from {} to {}".format(
            job_name, code, start_date, end_date
        ),
        ui_log=ui_log,
    )
    if start_date == end_date:
        return
    fetch_start = QA_util_get_next_day(start_date) if ref else start_date
    _insert_frame(coll, fetcher(str(code), fetch_start, end_date))


def _save_min_code(code, coll, fetcher, code_slice, job_name, ui_log=None):
    for type_ in _MIN_TYPES:
        ref = list(coll.find({"code": str(code)[:code_slice], "type": type_}))
        end_time = str(now_time())[:19]
        start_time = max(r["datetime"] for r in ref) if ref else "2015-01-01"
        QA_util_log_info(
            "{} Now Saving {} from {} to {} =={} ".format(
                job_name, code, start_time, end_time, type_
            ),
            ui_log=ui_log,
        )
        if start_time == end_time:
            continue
        frame = fetcher(str(code), start_time, end_time, type_)
        if frame is not None and len(frame) > 1:
            _insert_frame(coll, frame, skip_first=bool(ref))


def _save_many(codes, saver, workers=1, ui_log=None, ui_progress=None):
    import concurrent.futures
    import threading

    err = []
    total = len(codes)
    if workers > 1:
        _lock = threading.Lock()
        _done = [0]

        def _run(code):
            try:
                saver(code)
                with _lock:
                    _done[0] += 1
                    _log_progress(_done[0], total, ui_log, ui_progress)
                return None
            except Exception as error:
                QA_util_log_info(error, ui_log=ui_log)
                return str(code)

        with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as ex:
            results = list(ex.map(_run, codes))
        err = [r for r in results if r is not None]
    else:
        for item, code in enumerate(codes):
            _log_progress(item, total, ui_log, ui_progress)
            try:
                saver(code)
            except Exception as error:
                QA_util_log_info(error, ui_log=ui_log)
                err.append(str(code))
    if err:
        QA_util_log_info(" ERROR CODE \n ", ui_log=ui_log)
        QA_util_log_info(err, ui_log=ui_log)
    else:
        QA_util_log_info("SUCCESS", ui_log=ui_log)


def QA_SU_save_single_stock_day(code, client=DATABASE, ui_log=None):
    coll = client.stock_day
    _create_day_index(coll)
    _save_day_code(code, coll, QA_fetch_get_stock_day, "1990-01-01", 6, "UPDATE_STOCK_DAY", ui_log)


def QA_SU_save_stock_day(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.stock_day
    _create_day_index(coll)
    _save_many(
        _get_stock_list_with_fallback(client),
        lambda code: _save_day_code(code, coll, QA_fetch_get_stock_day, "1990-01-01", 6, "UPDATE_STOCK_DAY", ui_log),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_single_stock_min(code, client=DATABASE, ui_log=None, ui_progress=None):
    coll = client.stock_min
    _create_min_index(coll)
    _save_min_code(code, coll, QA_fetch_get_stock_min, 6, "##JOB03", ui_log)


def QA_SU_save_stock_min(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.stock_min
    _create_min_index(coll)
    _save_many(
        _get_stock_list_with_fallback(client),
        lambda code: _save_min_code(code, coll, QA_fetch_get_stock_min, 6, "##JOB03", ui_log),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_stock_transaction(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.stock_transaction
    _create_min_index(coll)
    _save_many(
        _get_stock_list_with_fallback(client),
        lambda code: _insert_frame(
            coll,
            QA_fetch_get_stock_transaction(
                str(code),
                "2019-01-01",
                str(now_time())[:10],
            ),
        ),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_stock_xdxr(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    stock_list = _get_stock_list_with_fallback(client)
    coll = client.stock_xdxr
    coll_adj = client.stock_adj
    coll.create_index([("code", pymongo.ASCENDING), ("date", pymongo.ASCENDING)], unique=True)
    coll_adj.create_index([("code", pymongo.ASCENDING), ("date", pymongo.ASCENDING)], unique=True)

    def save_one(code):
        QA_util_log_info("##JOB02 Now Saving XDXR INFO ==== {}".format(code), ui_log=ui_log)
        xdxr = QA_fetch_get_stock_xdxr(str(code))
        if xdxr is None or len(xdxr) < 1:
            return
        try:
            coll.insert_many(QA_util_to_json_from_pandas(xdxr), ordered=False)
        except Exception:
            pass
        data = QA_fetch_stock_day(str(code), "1990-01-01", str(datetime.date.today()), "pd")
        if data is None or len(data) < 1:
            return
        qfq = _QA_data_stock_to_fq(data, xdxr, "qfq")
        qfq = qfq.assign(date=qfq.date.apply(lambda value: str(value)[:10]))
        adjdata = QA_util_to_json_from_pandas(qfq.loc[:, ["date", "code", "adj"]])
        coll_adj.delete_many({"code": code})
        if adjdata:
            coll_adj.insert_many(adjdata)

    _save_many(stock_list, save_one, workers=workers, ui_log=ui_log, ui_progress=ui_progress)


def QA_SU_save_stock_list(client=DATABASE, ui_log=None, ui_progress=None):
    QA_util_log_info("##JOB08 Now Saving STOCK_LIST ====", ui_log=ui_log, ui_progress=ui_progress, ui_progress_int_value=5000)
    frame = QA_fetch_get_stock_list()
    payload = QA_util_to_json_from_pandas(frame)
    if payload:
        client.drop_collection("stock_list")
        coll = client.stock_list
        coll.create_index("code")
        coll.insert_many(payload)
    QA_util_log_info("完成股票列表获取", ui_log=ui_log, ui_progress=ui_progress, ui_progress_int_value=10000)


def QA_SU_save_stock_info(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    client.drop_collection("stock_info")
    coll = client.stock_info
    coll.create_index("code")
    _save_many(
        _get_stock_list_with_fallback(client),
        lambda code: _insert_frame(coll, QA_fetch_get_stock_info(str(code))),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_stock_block(client=DATABASE, ui_log=None, ui_progress=None):
    client.drop_collection("stock_block")
    coll = client.stock_block
    coll.create_index("code")
    _insert_frame(coll, QA_fetch_get_stock_block("opentdx"))
    _insert_frame(coll, QA_fetch_get_stock_block("tushare"))
    QA_util_log_info("完成股票板块获取=", ui_log=ui_log, ui_progress=ui_progress, ui_progress_int_value=10000)


def QA_SU_save_single_etf_day(code, client=DATABASE, ui_log=None):
    coll = client.etf_day
    _create_day_index(coll)
    _save_day_code(code, coll, QA_fetch_get_stock_day, "1990-01-01", 6, "UPDATE_ETF_DAY", ui_log)


def QA_SU_save_etf_day(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.etf_day
    _create_day_index(coll)
    _save_many(
        _codes_from_frame(QA_fetch_get_stock_list("etf")),
        lambda code: _save_day_code(code, coll, QA_fetch_get_stock_day, "1990-01-01", 6, "UPDATE_ETF_DAY", ui_log),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_single_etf_min(code, client=DATABASE, ui_log=None, ui_progress=None):
    coll = client.etf_min
    _create_min_index(coll)
    _save_min_code(code, coll, QA_fetch_get_stock_min, 6, "##JOB07", ui_log)


def QA_SU_save_etf_min(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.etf_min
    _create_min_index(coll)
    _save_many(
        _codes_from_frame(QA_fetch_get_stock_list("etf")),
        lambda code: _save_min_code(code, coll, QA_fetch_get_stock_min, 6, "##JOB07", ui_log),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_etf_list(client=DATABASE, ui_log=None, ui_progress=None):
    payload = QA_util_to_json_from_pandas(QA_fetch_get_stock_list(type_="etf"))
    if payload:
        client.drop_collection("etf_list")
        coll = client.etf_list
        coll.create_index("code")
        coll.insert_many(payload)


def QA_SU_save_single_index_day(code, client=DATABASE, ui_log=None):
    coll = client.index_day
    _create_day_index(coll)
    _save_day_code(code, coll, QA_fetch_get_index_day, "1990-01-01", 6, "UPDATE_INDEX_DAY", ui_log)


def QA_SU_save_index_day(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.index_day
    _create_day_index(coll)
    _save_many(
        _get_index_list_with_fallback(client),
        lambda code: _save_day_code(code, coll, QA_fetch_get_index_day, "1990-01-01", 6, "UPDATE_INDEX_DAY", ui_log),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_single_index_min(code, client=DATABASE, ui_log=None, ui_progress=None):
    coll = client.index_min
    _create_min_index(coll)
    _save_min_code(code, coll, QA_fetch_get_index_min, 6, "##JOB05", ui_log)


def QA_SU_save_index_min(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.index_min
    _create_min_index(coll)
    _save_many(
        _get_index_list_with_fallback(client),
        lambda code: _save_min_code(code, coll, QA_fetch_get_index_min, 6, "##JOB05", ui_log),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_index_transaction(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.index_transaction
    _create_min_index(coll)
    _save_many(
        _get_index_list_with_fallback(client),
        lambda code: _insert_frame(
            coll,
            QA_fetch_get_index_transaction(
                str(code),
                "2019-01-01",
                str(now_time())[:10],
            ),
        ),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_index_list(client=DATABASE, ui_log=None, ui_progress=None):
    payload = QA_util_to_json_from_pandas(QA_fetch_get_index_list())
    if payload:
        coll = client.index_list
        coll.create_index("code", unique=True)
        try:
            coll.insert_many(payload, ordered=False)
        except Exception:
            pass


def _future_codes(all_contracts):
    codes = _codes_from_frame(QA_fetch_get_future_list())
    if all_contracts:
        return codes
    return [code for code in codes if str(code)[-2:] in ["L8", "L9"]]


def QA_SU_save_single_future_day(code, client=DATABASE, ui_log=None, ui_progress=None):
    coll = client.future_day
    _create_day_index(coll)
    _save_day_code(code, coll, QA_fetch_get_future_day, "2001-01-01", 6, "UPDATE_FUTURE_DAY", ui_log)


def QA_SU_save_future_day(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.future_day
    _create_day_index(coll)
    _save_many(
        _future_codes(False),
        lambda code: _save_day_code(code, coll, QA_fetch_get_future_day, "2001-01-01", 6, "UPDATE_FUTURE_DAY", ui_log),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_future_day_all(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.future_day
    _create_day_index(coll)
    _save_many(
        _future_codes(True),
        lambda code: _save_day_code(code, coll, QA_fetch_get_future_day, "2001-01-01", 6, "UPDATE_FUTURE_DAY", ui_log),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_single_future_min(code, client=DATABASE, ui_log=None, ui_progress=None):
    coll = client.future_min
    _create_min_index(coll)
    _save_min_code(code, coll, QA_fetch_get_future_min, 6, "##JOB13", ui_log)


def QA_SU_save_future_min(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.future_min
    _create_min_index(coll)
    _save_many(
        _future_codes(False),
        lambda code: _save_min_code(code, coll, QA_fetch_get_future_min, 6, "##JOB13", ui_log),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_future_min_all(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.future_min
    _create_min_index(coll)
    _save_many(
        _future_codes(True),
        lambda code: _save_min_code(code, coll, QA_fetch_get_future_min, 6, "##JOB13", ui_log),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_future_list(client=DATABASE, ui_log=None, ui_progress=None):
    payload = QA_util_to_json_from_pandas(QA_fetch_get_future_list())
    if payload:
        coll = client.future_list
        coll.create_index("code", unique=True)
        try:
            coll.insert_many(payload, ordered=False)
        except Exception:
            pass


def QA_SU_save_single_bond_day(code, client=DATABASE, ui_log=None):
    coll = client.bond_day
    _create_day_index(coll)
    _save_day_code(code, coll, QA_fetch_get_bond_day, "1990-01-01", 6, "UPDATE_BOND_DAY", ui_log)


def QA_SU_save_bond_day(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.bond_day
    _create_day_index(coll)
    _save_many(
        _codes_from_frame(QA_fetch_get_bond_list()),
        lambda code: _save_day_code(code, coll, QA_fetch_get_bond_day, "1990-01-01", 6, "UPDATE_BOND_DAY", ui_log),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_single_bond_min(code, client=DATABASE, ui_log=None, ui_progress=None):
    coll = client.bond_min
    _create_min_index(coll)
    _save_min_code(code, coll, QA_fetch_get_bond_min, 6, "##JOB07", ui_log)


def QA_SU_save_bond_min(client=DATABASE, ui_log=None, ui_progress=None, workers=1):
    coll = client.bond_min
    _create_min_index(coll)
    _save_many(
        _codes_from_frame(QA_fetch_get_bond_list()),
        lambda code: _save_min_code(code, coll, QA_fetch_get_bond_min, 6, "##JOB07", ui_log),
        workers=workers,
        ui_log=ui_log,
        ui_progress=ui_progress,
    )


def QA_SU_save_bond_list(client=DATABASE, ui_log=None, ui_progress=None):
    payload = QA_util_to_json_from_pandas(QA_fetch_get_bond_list())
    if payload:
        client.drop_collection("bond_list")
        coll = client.bond_list
        coll.create_index("code")
        coll.insert_many(payload)
