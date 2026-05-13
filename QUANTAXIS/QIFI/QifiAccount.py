import datetime
import numbers
import traceback
import uuid
import threading
import time
import copy
import queue
from decimal import Decimal, ROUND_HALF_UP

import bson
import numpy as np
import pandas as pd
import pymongo
from pymongo import message
from qaenv import mongo_ip, clickhouse_ip, clickhouse_password, clickhouse_port, clickhouse_user
from QUANTAXIS.QAMarket.market_preset import MARKET_PRESET
from QUANTAXIS.QAMarket.QAOrder import ORDER_DIRECTION
from QUANTAXIS.QAMarket.QAPosition import QA_Position
import clickhouse_driver


def parse_orderdirection(od):
    direction = ''
    offset = ''

    if od in [1, 2, 3, 4]:
        direction = 'BUY'
    elif od in [-1, -2, -3, -4]:
        direction = 'SELL'
    if abs(od) == 2 or od == 1:
        offset = 'OPEN'
    elif abs(od) == 3 or od == -1:
        offset = 'CLOSE'
    elif abs(od) == 4:
        offset = 'CLOSETODAY'

    return direction, offset


class QIFI_Account():
    _EPSILON = Decimal("0.0001")
    _MONEY_QUANT = Decimal("0.0001")
    _REQUIRED_QIFI_ACCOUNT_FIELDS = (
        "user_id",
        "pre_balance",
        "deposit",
        "withdraw",
        "WithdrawQuota",
        "close_profit",
        "static_balance",
    )
    _REQUIRED_QIFI_POSITION_FIELDS = (
        "account_cookie",
        "portfolio_cookie",
        "username",
        "frozen",
        "moneypreset",
        "moneypresetLeft",
        "volume_long_today",
        "volume_long_his",
        "volume_short_today",
        "volume_short_his",
        "volume_long_frozen_his",
        "volume_long_frozen_today",
        "volume_short_frozen_his",
        "volume_short_frozen_today",
        "margin_long",
        "margin_short",
        "open_price_long",
        "open_price_short",
        "position_price_long",
        "position_price_short",
        "open_cost_long",
        "open_cost_short",
        "position_cost_long",
        "position_cost_short",
        "position_id",
        "market_type",
        "exchange_id",
        "trades",
        "orders",
        "name",
    )

    def __init__(self, username, password, model="SIM", broker_name="QAPaperTrading", portfolioname='QAPaperTrade',
                 trade_host=mongo_ip, init_cash=1000000, taskid=str(uuid.uuid4()), nodatabase=False, dbname='mongodb',
                 clickhouse_ip=clickhouse_ip, clickhouse_port=clickhouse_port, clickhouse_user=clickhouse_user,
                 clickhouse_password=clickhouse_password, strict_sync=False, strict_code_format=True):
        """Initial
        QIFI Account是一个基于 DIFF/ QIFI/ QAAccount后的一个实盘适用的Account基类


        1. 兼容多持仓组合
        2. 动态计算权益

        使用 model = SIM/ REAL来切换

        qifiaccount 不去区分你的持仓是股票还是期货, 因此你可以实现跨市场的交易持仓管理
        nodatabase 离线模式


        source_id ==> 基于 user_id / tradeday 区分
        """
        self.user_id = username
        self.username = username
        self.password = password
        self.qifi_id = str(uuid.uuid4())
        self.source_id = "QIFI_Account"  # 识别号
        self.market_preset = MARKET_PRESET()
        # 指的是 Account所属的账户编组(实时的时候的账户观察组)
        self.portfolio = portfolioname
        self.model = model

        self.broker_name = broker_name    # 所属期货公司/ 模拟的组
        self.investor_name = ""  # 账户所属人(实盘的开户人姓名)
        self.bank_password = ""
        self.capital_password = ""
        self.wsuri = ""
        self.commission_fee = 0.0015
        self.bank_id = "QASIM"
        self.bankname = "QASIMBank"

        self.trade_host = trade_host

        self.pub_host = ""
        #self.trade_host = ""
        self.last_updatetime = ""
        self.status = 200
        self._trading_day = ""
        self.init_cash = init_cash
        self.pre_balance = 0
        self.datetime = ""
        self.static_balance = 0

        self.deposit = 0  # 入金
        self.withdraw = 0  # 出金
        self.withdrawQuota = 0  # 可取金额
        self.close_profit = 0
        self.premium = 0  # 本交易日内交纳的期权权利金
        self.event_id = 0
        self.taskid = taskid
        self.money = 0
        # QIFI 协议
        self.transfers = {}
        self.schedule = {}

        self.banks = {}

        self.frozen = {}

        self.event = {}
        self.positions = {}
        self.trades = {}
        self.orders = {}
        self.market_preset = MARKET_PRESET()
        self.nodatabase = nodatabase
        self.dbname = dbname
        self._clickhouse_ip = clickhouse_ip
        self._clickhouse_port = clickhouse_port
        self._clickhouse_user = clickhouse_user
        self._clickhouse_password = clickhouse_password
        self.verbose = False
        self.last_sync_error = None
        self.last_sync_success = None
        self.strict_sync = strict_sync
        self.strict_code_format = strict_code_format
        self.persist_pending = False
        self._lock = threading.RLock()
        self._order_timeout_seconds = 30
        self._partial_fill_timeout_seconds = 30
        self._order_check_failure_reason = None
        self._persist_queue = queue.Queue(maxsize=1024)
        self._persist_worker = None
        self._persist_stop_event = threading.Event()
        self._background_worker = None
        self._background_worker_stop_event = threading.Event()
        self._background_check_interval = 1.0

    @classmethod
    def _to_decimal(cls, value):
        if isinstance(value, Decimal):
            return value
        return Decimal(str(value))

    @classmethod
    def _quantize_money(cls, value):
        return cls._to_decimal(value).quantize(cls._MONEY_QUANT, rounding=ROUND_HALF_UP)

    @classmethod
    def _decimal_to_float(cls, value):
        return float(cls._quantize_money(value))

    def _sanitize_message(self, message):
        sanitized = dict(message)
        sanitized.pop("password", None)
        sanitized.pop("bank_password", None)
        sanitized.pop("capital_password", None)
        return sanitized

    def _build_message(self, include_sensitive=False):
        payload = {
            "account_cookie": self.user_id,
            "databaseip": self.trade_host,
            "model": self.model,
            "ping_gap": 5,
            "portfolio": self.portfolio,
            "broker_name": self.broker_name,
            "bankid": self.bank_id,
            "investor_name": self.investor_name,
            "money": self.money,
            "pub_host": self.pub_host,
            "trade_host": self.trade_host,
            "taskid": self.taskid,
            "sourceid": self.source_id,
            "updatetime": str(self.last_updatetime),
            "wsuri": self.wsuri,
            "bankname": self.bankname,
            "trading_day": str(self.trading_day),
            "status": self.status,
            "accounts": self.account_msg,
            "trades": self.trades,
            "positions": self.position_msg,
            "orders": self.orders,
            "event": self.event,
            "transfers": self.transfers,
            "banks": self.banks,
            "frozen": self.frozen,
            "settlement": {},
            "sync_state": self.sync_state,
        }
        if include_sensitive:
            payload["password"] = self.password
            payload["capital_password"] = self.capital_password
            payload["bank_password"] = self.bank_password
        return payload

    def full_message(self):
        return self._build_message(include_sensitive=True)

    def _snapshot_state(self):
        return (
            copy.deepcopy(self.message),
            copy.deepcopy(self.account_msg),
            str(self.trading_day),
        )

    def _ensure_persist_worker(self):
        if self.nodatabase or self.model == "BACKTEST":
            return
        if self._persist_worker is None or not self._persist_worker.is_alive():
            self._persist_stop_event.clear()
            self._persist_worker = threading.Thread(
                target=self._persist_loop,
                name="QIFIAccountPersist",
                daemon=True,
            )
            self._persist_worker.start()

    def _ensure_background_worker(self):
        if self.nodatabase or self.model == "BACKTEST":
            return
        if self._background_worker is None or not self._background_worker.is_alive():
            self._background_worker_stop_event.clear()
            self._background_worker = threading.Thread(
                target=self._background_monitor_loop,
                name="QIFIAccountMonitor",
                daemon=True,
            )
            self._background_worker.start()

    def shutdown_background_workers(self):
        self._persist_stop_event.set()
        self._background_worker_stop_event.set()
        self.flush_persist_queue(timeout=1)

    def flush_persist_queue(self, timeout=5):
        if self._persist_worker is None:
            return True
        deadline = time.time() + timeout
        while time.time() < deadline:
            if self._persist_queue.unfinished_tasks == 0 and self._persist_queue.empty():
                return True
            time.sleep(0.05)
        return self._persist_queue.unfinished_tasks == 0 and self._persist_queue.empty()

    def _background_monitor_loop(self):
        while not self._background_worker_stop_event.is_set():
            try:
                self.run_background_tasks_once()
            except Exception as e:
                self.log('background monitor failed: {}'.format(e))
            self._background_worker_stop_event.wait(self._background_check_interval)

    def _persist_loop(self):
        while not self._persist_stop_event.is_set():
            try:
                context, message_snapshot, account_snapshot, trading_day_snapshot = self._persist_queue.get(timeout=0.5)
            except queue.Empty:
                continue
            try:
                self._write_snapshot_to_storage(
                    message_snapshot,
                    account_snapshot,
                    trading_day_snapshot,
                )
                self.last_sync_error = None
                self.last_sync_success = True
                if self._persist_queue.empty():
                    self.persist_pending = False
            except Exception:
                self.last_sync_success = False
                self.persist_pending = True
                self.last_sync_error = traceback.format_exc()
                self.log('async persist failed during {}: {}'.format(context, self.last_sync_error))
            finally:
                self._persist_queue.task_done()

    def _write_snapshot_to_storage(self, message_snapshot, account_snapshot, trading_day_snapshot):
        if self.nodatabase:
            return message_snapshot
        if self.dbname in ['ck', 'clickhouse']:
            self.save_ck()
        else:
            if self.model == "BACKTEST":
                self.db = pymongo.MongoClient(self.trade_host).quantaxis
                self.db.history.update_one(
                    {'account_cookie': self.user_id, 'trading_day': trading_day_snapshot},
                    {'$set': message_snapshot},
                    upsert=True
                )
            else:
                self.db.account.update_one(
                    {'account_cookie': self.user_id},
                    {'$set': message_snapshot},
                    upsert=True
                )
                self.db.hisaccount.insert_one(
                    {'updatetime': self.dtstr, 'account_cookie': self.user_id, 'accounts': account_snapshot}
                )
        return message_snapshot

    def _persist_state(self, context='sync', force_sync=False):
        with self._lock:
            message_snapshot, account_snapshot, trading_day_snapshot = self._snapshot_state()

        if self.nodatabase:
            self.persist_pending = False
            self.last_sync_success = True
            self.last_sync_error = None
            return self.message

        if force_sync or self.strict_sync or self.model == "BACKTEST":
            for attempt, delay in enumerate((0, 1, 2, 4), start=1):
                try:
                    if delay:
                        time.sleep(delay)
                    self._write_snapshot_to_storage(
                        message_snapshot,
                        account_snapshot,
                        trading_day_snapshot,
                    )
                    self.last_sync_error = None
                    self.last_sync_success = True
                    self.persist_pending = False
                    return message_snapshot
                except Exception:
                    self.last_sync_success = False
                    self.persist_pending = True
                    self.last_sync_error = traceback.format_exc()
                    self.log('sync attempt {} failed during {}: {}'.format(attempt, context, self.last_sync_error))
            return None

        self._ensure_persist_worker()
        try:
            self._persist_queue.put_nowait(
                (context, message_snapshot, account_snapshot, trading_day_snapshot)
            )
            self.persist_pending = True
            self.last_sync_success = True
            self.last_sync_error = None
        except queue.Full:
            self.log('persist queue full during {}'.format(context))
        return message_snapshot

    def initial(self):
        if not self.nodatabase:
            if self.dbname in ['ck', 'clickhouse']:
                self.db = clickhouse_driver.Client(host=self._clickhouse_ip, port=self._clickhouse_port,
                                                    user=self._clickhouse_user, password=self._clickhouse_password,
                                                    database='qifi',
                                                    settings={
                                                        'insert_block_size': 100000000},
                                                    compression=True)
                self.reload_ck()

            else:

                if self.model == "BACKTEST":
                    self.db = pymongo.MongoClient(
                        self.trade_host).quantaxis
                else:
                    self.db = pymongo.MongoClient(
                        self.trade_host).QAREALTIME
                self.reload()
        else:
            """
            非数据库模式  不用 reload
            """
            self.log('当前为 QIFIAccount::非数据库模式, 适用于测试/二次开发')

        if self.pre_balance == 0 and self.balance == 0 and self.model != "REAL":
            self.log('Create new Account')
            if self.model == "BACKTEST":
                self.create_backtestaccount()
            else:
                self.create_simaccount()
        self.sync()
        self._ensure_persist_worker()
        self._ensure_background_worker()



    def save_ck(self):
        for tablename  in ['accounts', 'positions', 'orders', 'trades', 'banks', 'qifi']:
            self.log(tablename)

            res = self.get_for_ck(tablename)
            if res and len(res)>0:

                self.db.execute('INSERT INTO qifi.{} VALUES'.format(tablename), res)
                self.db.execute('OPTIMIZE TABLE qifi.{}'.format(tablename))
    def reload_ck(self):
        if self.model.upper() in ['REAL', 'SIM']:
            res = self.db.execute("select * from qifi.qifi where account_cookie='{}' and trading_day='{}' limit 1".format(self.user_id, self.trading_day))
            if len(res) ==1:
                self.qifi_id =res['qifi_id']



    @property
    def trading_day(self):
        if self.model == "BACKTEST":
            return str(self.datetime)[0:10]
        else:
            return self._trading_day

    def _require_mapping(self, field_name, value):
        if not isinstance(value, dict):
            raise TypeError("{} must be a dict".format(field_name))
        return value

    def _require_string(self, field_name, value):
        if not isinstance(value, str):
            raise TypeError("{} must be a str".format(field_name))
        if value == "":
            raise ValueError("{} cannot be empty".format(field_name))
        return value

    def _require_numeric(self, field_name, value):
        if not isinstance(value, numbers.Real):
            raise TypeError("{} must be numeric".format(field_name))
        return value

    def _is_unknown_derivative_code(self, code, exchange_id=None):
        if not isinstance(code, str):
            return False
        instrument_id = code.split('.')[-1]
        if not any(ch.isalpha() for ch in instrument_id):
            return False
        preset = self.market_preset.get_code(instrument_id)
        resolved_exchange = exchange_id or preset.get('exchange')
        return (
            preset.get('name') == 'default' and
            resolved_exchange == 'stock_cn'
        )

    def _validate_qifi_position_message(self, position_key, position, account_cookie, portfolio_cookie):
        position = self._require_mapping(
            "positions[{!r}]".format(position_key),
            position
        )
        instrument_id = position.get('instrument_id') or position.get('code')
        if not instrument_id:
            raise ValueError(
                "positions[{!r}] missing instrument_id/code".format(position_key)
            )
        exchange_id = self._require_string(
            "positions[{!r}].exchange_id".format(position_key),
            position.get('exchange_id')
        )
        if self._is_unknown_derivative_code(instrument_id, exchange_id=exchange_id):
            raise ValueError(
                "positions[{!r}] uses unknown derivative instrument {!r}".format(
                    position_key, instrument_id
                )
            )
        missing_fields = [
            field for field in self._REQUIRED_QIFI_POSITION_FIELDS if field not in position
        ]
        if missing_fields:
            raise ValueError(
                "positions[{!r}] missing fields: {}".format(
                    position_key, ", ".join(missing_fields)
                )
            )
        if position['account_cookie'] != account_cookie:
            raise ValueError(
                "positions[{!r}].account_cookie mismatch".format(position_key)
            )
        if position['portfolio_cookie'] != portfolio_cookie:
            raise ValueError(
                "positions[{!r}].portfolio_cookie mismatch".format(position_key)
            )
        if position['username'] != account_cookie:
            raise ValueError(
                "positions[{!r}].username mismatch".format(position_key)
            )
        if not isinstance(position['frozen'], dict):
            raise TypeError(
                "positions[{!r}].frozen must be a dict".format(position_key)
            )
        if not isinstance(position['trades'], list):
            raise TypeError(
                "positions[{!r}].trades must be a list".format(position_key)
            )
        if not isinstance(position['orders'], dict):
            raise TypeError(
                "positions[{!r}].orders must be a dict".format(position_key)
            )

    def _validate_qifi_message(self, message):
        if not isinstance(message, dict):
            raise TypeError("QIFI message must be a dict")

        account_cookie = self._require_string(
            'account_cookie',
            message.get('account_cookie')
        )
        accounts = self._require_mapping('accounts', message.get('accounts'))
        positions = self._require_mapping('positions', message.get('positions'))
        orders = self._require_mapping('orders', message.get('orders'))
        trades = self._require_mapping('trades', message.get('trades'))

        if 'portfolio' not in message:
            raise ValueError("portfolio is required in QIFI message")
        portfolio_cookie = self._require_string('portfolio', message.get('portfolio'))

        missing_account_fields = [
            field for field in self._REQUIRED_QIFI_ACCOUNT_FIELDS if field not in accounts
        ]
        if missing_account_fields:
            raise ValueError(
                "accounts missing fields: {}".format(", ".join(missing_account_fields))
            )
        if accounts['user_id'] != account_cookie:
            raise ValueError("accounts.user_id must match account_cookie")
        if account_cookie != self.user_id:
            raise ValueError("account_cookie does not match current account")
        if portfolio_cookie != self.portfolio:
            raise ValueError("portfolio does not match current account")

        for field in self._REQUIRED_QIFI_ACCOUNT_FIELDS[1:]:
            self._require_numeric("accounts.{}".format(field), accounts[field])

        for optional_dict_field in ('event', 'transfers', 'banks', 'frozen'):
            if optional_dict_field in message and not isinstance(message[optional_dict_field], dict):
                raise TypeError("{} must be a dict".format(optional_dict_field))

        if 'money' in message:
            self._require_numeric('money', message['money'])
        if 'taskid' in message:
            self._require_string('taskid', message['taskid'])
        if 'sourceid' in message:
            self._require_string('sourceid', message['sourceid'])
        if 'status' in message and not isinstance(message['status'], int):
            raise TypeError("status must be an int")
        if 'wsuri' in message and message['wsuri'] is not None and not isinstance(message['wsuri'], str):
            raise TypeError("wsuri must be a str")
        if 'trading_day' in message and message['trading_day'] is not None and not isinstance(message['trading_day'], str):
            raise TypeError("trading_day must be a str")

        for position_key, position in positions.items():
            self._validate_qifi_position_message(
                position_key,
                position,
                account_cookie=account_cookie,
                portfolio_cookie=portfolio_cookie
            )

        for collection_name, collection in (('orders', orders), ('trades', trades)):
            for item_key, item in collection.items():
                if not isinstance(item, dict):
                    raise TypeError(
                        "{}[{!r}] must be a dict".format(collection_name, item_key)
                    )

        return message

    def reload(self):
        if self.model.upper() in ['REAL', 'SIM']:
            message = self.db.account.find_one(
                {'account_cookie': self.user_id})

            time = datetime.datetime.now()
            # resume/settle

            if time.hour <= 15:
                self._trading_day = time.date()
            else:
                if time.weekday() in [0, 1, 2, 3]:
                    self._trading_day = time.date() + datetime.timedelta(days=1)
                elif time.weekday() in [4, 5, 6]:
                    self._trading_day = time.date() + datetime.timedelta(days=(7-time.weekday()))
            if message is not None:
                self._load_qifi_message(message)
                self.reconcile_pending_orders()

                self.on_reload()

                if message.get('trading_day', '') == str(self._trading_day):
                    # reload
                    pass

                else:
                    # settle
                    self.settle()

    def _load_qifi_message(self, message):
        message = self._validate_qifi_message(message)
        accpart = message['accounts']

        self.money = message.get('money', self.money)
        self.source_id = message.get('sourceid', self.source_id)

        self.pre_balance = accpart.get('pre_balance', self.pre_balance)
        self.deposit = accpart.get('deposit', self.deposit)
        self.withdraw = accpart.get('withdraw', self.withdraw)
        self.withdrawQuota = accpart.get('WithdrawQuota', self.withdrawQuota)
        self.close_profit = accpart.get('close_profit', self.close_profit)
        self.static_balance = accpart.get('static_balance', self.static_balance)
        self.event = message.get('event') or {}
        self.trades = message['trades']
        self.transfers = message.get('transfers') or {}
        self.orders = message['orders']
        self.frozen = message.get('frozen') or {}
        self.taskid = message.get('taskid', str(uuid.uuid4()))

        self.positions = {}
        positions = message['positions']
        for position in positions.values():
            loaded_position = QA_Position().loadfrommessage(position)
            key = '{}.{}'.format(
                position.get('exchange_id'),
                position.get('instrument_id')
            )
            self.positions[key] = loaded_position

        self.banks = message.get('banks') or {}
        self.status = message.get('status', self.status)
        self.wsuri = message.get('wsuri', self.wsuri)
        trading_day = message.get('trading_day')
        if trading_day is not None:
            self._trading_day = trading_day
        self.persist_pending = False
        return self

    def create_fromQIFI(self, message):
        self._load_qifi_message(message)
        self.on_reload()
        return self

    def order_rule(self):
        """
        订单流控
        """
        pass

    def batch_buy(self, codedf: pd.Series, datetime: str, totalamount: float = 1000000, model: enumerate = 'avg_money'):
        """
        批量调仓接口

        codedf: pd.Series

            Series.index -> code
            Series.value -> price


        totalamount: 总买入金额

        model Enum
            'avg_money': 等市值买入
            'avg_amount': 等股数买入(买入总金额==totalamount)
        """
        if model == 'avg_money':
            moneyper = totalamount / len(codedf)
            amount = (moneyper/codedf).apply(lambda x: (int(100/x)*100)
                                             if int(100/x) > 0 else 100)
        elif model == 'avg_amount':
            amountx = int(totalamount/(100*codedf.sum()))
            if amountx == 0:
                return False
            else:
                amount = codedf.apply(lambda x: amountx*100)
        orderres = pd.concat([codedf, amount], axis=1)
        orderres.columns = ['price', 'amount']
        res = orderres.assign(datetime=datetime).apply(lambda x: self.send_order(
            code=x.index, amount=x.amount, price=x.price, towards=1, datetime=x.datetime))
        return res

    def update_qifiid(self, val:dict):
        val['qifi_id'] = self.qifi_id
        return val
    def get_for_ck(self, name):
        """
        name should be in
        ['accounts', 'positions', 'orders', 'trades', 'banks', 'qifi']
        """
        if name == 'accounts':
            return [self.update_qifiid(self.account_msg)]
        elif name == 'orders':
            """

            "account_cookie": self.user_id,
                "user_id": self.user_id,
                "instrument_id": code,
                "towards": int(towards),
                "exchange_id": self.market_preset.get_exchange(code),

                "volume": int(amount),
                "price": float(price),
                "order_id": order_id,
                "seqno": self.event_id,
                "direction": direction,
                "offset": offset,
                "volume_orign": int(amount),
                "price_type": "LIMIT",
                "limit_price": float(price),
                "time_condition": "GFD",
                "volume_condition": "ANY",
                "insert_date_time": self.transform_dt(self.dtstr),
                'order_time': self.dtstr,
                "exchange_order_id": str(uuid.uuid4()),
                "status": "ALIVE",
                "volume_left": int(amount),
                "last_msg": "已报"
            qifi_id          String,
            seqno             Int32,
            user_id           String,
            order_id          String,
            exchange_id       String,
            instrument_id     String,
            direction         String,
            offset            String,
            volume_orign      Float64,
            price_type        String,
            limit_price       Float64,
            time_condition    String,
            insert_date_time  Int64,
            exchange_order_id String,
            order_time        String,
            status            String,
            volume_left       Float64,
            volume_condition  String,
            last_msg          String"""
            res =  list(self.orders.values())
            if len(res)>0:
                res = [self.update_qifiid(i) for i in res]
                return res
            else:
                return []
        elif name == 'trades':
            res =  list(self.trades.values())
            if len(res)>0:
                res = [self.update_qifiid(i) for i in res]
            return res

        elif name == 'positions':
            res= list(self.position_msg.values())
            if len(res)>0:
                res = [self.update_qifiid(i) for i in res]
                return res
            else:
                return []
        elif name == 'banks':
            res= list(self.banks.values())
            if len(res)>0:
                res = [self.update_qifiid(i) for i in res]
            return res
        elif name == 'qifi':

            """
            
                account_cookie   String,
                bank_password   String,
                qifi_id          String,
                bankid           String,
                bankname         String,
                broker_name      String,
                capital_password String,
                eventmq_ip       String,
                investor_name    String,
                money            Float64,
                password         String,
                ping_gap         Int32,
                portfolio        String,
                pub_host         String,
                taskid           String,
                trade_host       String,
                updatetime       String,
                wsuri            String,
                trading_day      String,
                status           Int32,
                databaseip       String"""
            return [{
                "account_cookie": self.user_id,
                "databaseip": self.trade_host,
                'qifi_id': self.qifi_id,
                "ping_gap": 5,
                "eventmq_ip": self.trade_host,
                "portfolio": self.portfolio,
                "broker_name": self.broker_name,  # // 接入商名称
                "bankid": self.bank_id,  # // 银行id
                "investor_name": self.investor_name,  # // 开户人名称
                "money": self.money,         # // 当前可用现金
                "pub_host": self.pub_host,
                "trade_host": self.trade_host,
                "taskid": self.taskid,
                "updatetime": str(self.last_updatetime),
                "wsuri": self.wsuri,
                "bankname": self.bankname,
                "trading_day": str(self.trading_day),
                "status": self.status,
            }]
    def sync(self):
        self.on_sync()
        return self._persist_state(context='sync', force_sync=True)

    def _mark_persist_pending(self):
        if not self.nodatabase:
            self.persist_pending = True

    def _sync_or_raise(self, context='sync'):
        result = self._persist_state(context=context, force_sync=self.strict_sync)
        if self.strict_sync and result is None:
            raise RuntimeError(
                "QIFI sync failed during {}: {}".format(
                    context, self.last_sync_error or 'unknown error'
                )
            )
        return result

    def settle(self):
        self.log('settle')
        self._sync_or_raise('settle')

        self.pre_balance += (self.deposit - self.withdraw + self.close_profit)
        self.static_balance = self.pre_balance

        self.close_profit = 0
        self.deposit = 0  # 入金
        self.withdraw = 0  # 出金
        self.premium = 0
        self.money += self.frozen_margin

        self.orders = {}
        self.frozen = {}
        self.trades = {}
        self.transfers = {}
        self.event = {}
        self.event_id = 0

        for item in self.positions.values():
            item.settle()

        # sell first >> second buy ==> for make sure have enough cash
        buy_order_sche = []
        for order in self.schedule.values():
            if order['towards'] > 0:
                # buy order
                buy_order_sche.append(order)
            else:
                self.send_order(order['code'], order['amount'],
                                order['price'], order['towards'], order['order_id'])
        for order in buy_order_sche:
            self.send_order(order['code'], order['amount'],
                            order['price'], order['towards'], order['order_id'])
        self.schedule = {}
        self.qifi_id = str(uuid.uuid4())

    def on_sync(self):
        pass

    def on_reload(self):
        pass

    def query_external_orders(self):
        return []

    def _release_open_order_frozen(self, order_id):
        frozen = self.frozen.get(order_id, {'order_id': order_id, 'money': 0, 'price': 0, 'coeff': 1})
        refund = self._to_decimal(frozen.get('money', 0))
        self.money = self._decimal_to_float(self._to_decimal(self.money) + refund)
        frozen['amount'] = 0
        frozen['money'] = 0
        frozen.setdefault('coeff', 1)
        self.frozen[order_id] = frozen

    def fail_order(self, order_id, reason=None):
        with self._lock:
            od = self.orders.get(order_id)
            if od is None:
                self.log('fail_order ignored missing order {}'.format(order_id))
                return False

            self._mark_persist_pending()
            remaining_volume = od.get('volume_left', 0)
            od['last_msg'] = '拒单' if reason is None else str(reason)
            od['status'] = "FAILED"
            od['volume_left'] = 0
            if od['offset'] in ['CLOSE', 'CLOSETODAY']:
                pos = self.positions.get(od['exchange_id'] + '.' + od['instrument_id'])
                if pos is not None:
                    if od['direction'] == 'BUY':
                        pos.volume_short_frozen_today = max(0, pos.volume_short_frozen_today - remaining_volume)
                    else:
                        pos.volume_long_frozen_today = max(0, pos.volume_long_frozen_today - remaining_volume)
            else:
                self._release_open_order_frozen(order_id)
            self.orders[order_id] = od
            if self.model != 'BACKTEST':
                self._sync_or_raise('fail_order')
            return True

    def reconcile_pending_orders(self, external_orders=None):
        external_orders = self.query_external_orders() if external_orders is None else external_orders
        if external_orders is None:
            return []
        if not isinstance(external_orders, list):
            raise TypeError('external_orders must be a list')
        external_by_id = {
            item.get('order_id'): item for item in external_orders
            if isinstance(item, dict) and item.get('order_id')
        }
        actions = []
        for local_order in list(self.open_orders):
            order_id = local_order.get('order_id')
            external = external_by_id.get(order_id)
            if external is None:
                if self.cancel_order(order_id):
                    actions.append('cancel:{}'.format(order_id))
                continue

            external_status = str(external.get('status', '')).upper()
            external_volume_left = int(external.get('volume_left', local_order.get('volume_left', 0)))
            local_volume_left = int(local_order.get('volume_left', 0))
            traded_amount = max(0, local_volume_left - external_volume_left)
            if traded_amount > 0:
                self.receive_deal(
                    local_order['instrument_id'],
                    trade_price=external.get('price', local_order.get('limit_price', local_order.get('price', 0))),
                    trade_amount=traded_amount,
                    trade_towards=local_order['towards'],
                    trade_time=external.get('trade_time', self.dtstr),
                    order_id=order_id,
                    trade_id=external.get('trade_id', 'reconcile-{}-{}'.format(order_id, traded_amount)),
                )
                actions.append('deal:{}:{}'.format(order_id, traded_amount))

            if external_status in ['CANCEL', 'CANCELLED', 'CANCELED']:
                if self.cancel_order(order_id):
                    actions.append('cancel:{}'.format(order_id))
            elif external_status in ['FAILED', 'REJECTED']:
                if self.fail_order(order_id, external.get('last_msg', external_status)):
                    actions.append('fail:{}'.format(order_id))
        return actions

    @property
    def dtstr(self):
        if self.model == "BACKTEST":
            return self.datetime.replace('.', '_')
        else:
            return str(datetime.datetime.now()).replace('.', '_')

    def _apply_deposit(self, money):
        self._mark_persist_pending()
        self.deposit += money
        self.money += money
        self.transfers[str(self.event_id)] = {
            "datetime": 433241234123,  # // 转账时间, epoch nano
            "currency": "CNY",  # 币种
            "amount": money,  # 涉及金额
            "error_id": 0,  # 转账结果代码
            "error_msg": "成功",  # 转账结果代码
        }
        self.event[self.dtstr] = "转账成功 {}".format(money)

    def ask_deposit(self, money):
        self._apply_deposit(money)
        if self.model != "BACKTEST":
            self._sync_or_raise('ask_deposit')

    def _apply_withdraw(self, money):
        if self.withdrawQuota > money:
            self._mark_persist_pending()
            self.withdrawQuota -= money
            self.withdraw += money
            self.transfers[str(self.event_id)] = {
                "datetime": 433241234123,  # // 转账时间, epoch nano
                "currency": "CNY",  # 币种
                "amount": -money,  # 涉及金额
                "error_id": 0,  # 转账结果代码
                "error_msg": "成功",  # 转账结果代码
            }
            self.event[self.dtstr] = "转账成功 {}".format(-money)
            return True
        else:
            self.event[self.dtstr] = "转账失败: 余额不足 left {}  ask {}".format(
                self.withdrawQuota, money)
            return False

    def ask_withdraw(self, money):
        success = self._apply_withdraw(money)
        if success and self.model != "BACKTEST":
            self._sync_or_raise('ask_withdraw')
        return success

    def create_simaccount(self):
        self._mark_persist_pending()
        self._trading_day = str(datetime.date.today())
        self.wsuri = "ws://www.yutiansut.com:7988"
        self.pre_balance = 0
        self.static_balance = 0
        self.deposit = 0  # 入金
        self.withdraw = 0  # 出金
        self.withdrawQuota = 0  # 可取金额
        self.user_id = self.user_id
        self.password = self.password
        self.money = 0
        self.close_profit = 0
        self.event_id = 0
        self.transfers = {}
        self.banks = {}
        self.event = {}
        self.positions = {}
        self.trades = {}
        self.orders = {}
        self.banks[str(self.bank_id)] = {
            "id": self.bank_id,
            "name": self.bankname,
            "bank_account": "",
            "fetch_amount": 0.0,
            "qry_count": 0
        }
        self._apply_deposit(self.init_cash)
        self._ensure_background_worker()
        self._ensure_persist_worker()

    def create_backtestaccount(self):
        """
        生成一个回测的账户

        回测账户的核心事件轴是数据的datetime, 基于数据的datetime来进行账户的更新


        """
        self._mark_persist_pending()
        self._trading_day = ""
        self.pre_balance = self.init_cash
        self.static_balance = self.init_cash
        self.deposit = 0  # 入金
        self.withdraw = 0  # 出金
        self.withdrawQuota = 0  # 可取金额
        self.user_id = self.user_id
        self.password = self.password
        self.money = self.init_cash
        self.close_profit = 0
        self.event_id = 0
        self.transfers = {}
        self.banks = {}
        self.event = {}
        self.positions = {}
        self.trades = {}
        self.orders = {}
        self.banks[str(self.bank_id)] = {
            "id": self.bank_id,
            "name": self.bankname,
            "bank_account": "",
            "fetch_amount": 0.0,
            "qry_count": 0
        }

        # self.ask_deposit(self.init_cash)
        self._ensure_background_worker()
        self._ensure_persist_worker()

    def add_position(self, position):

        if position.instrument_id not in self.positions.keys():
            self.positions[position.exchange_id +
                           '.'+position.instrument_id] = position
            return 0
        else:
            return 1

    def drop_position(self, position):
        pass

    def log(self, message):
        if self.verbose:
            print(message)
        #self.event[self.dtstr] = message

    @property
    def open_orders(self):
        return [item for item in self.orders.values() if item['volume_left'] > 0]

    @property
    def message(self):
        return self._sanitize_message(self._build_message(include_sensitive=False))

    @property
    def account_msg(self):
        balance = self.balance
        return {
            "user_id": self.user_id,
            "currency": "CNY",
            "pre_balance": self.pre_balance,
            "deposit": self.deposit,
            "withdraw": self.withdraw,
            "WithdrawQuota": self.withdrawQuota,
            "close_profit": self.close_profit,
            "commission": self.commission,
            "premium": self.premium,
            "static_balance": self.static_balance,
            "position_profit": self.position_profit,
            "float_profit": self.float_profit,
            "balance": self.balance,
            "margin": self.margin,
            "frozen_margin": self.frozen_margin,
            "frozen_commission": 0.0,
            "frozen_premium": 0.0,
            "available": self.available,
            "risk_ratio": 0.0 if balance == 0 else 1 - self.available / balance
        }

    @property
    def sync_state(self):
        return {
            "strict_sync": self.strict_sync,
            "last_sync_success": self.last_sync_success,
            "persist_pending": self.persist_pending,
            "last_sync_error": self.last_sync_error,
        }

    @property
    def position_msg(self):
        return dict(zip(self.positions.keys(), [item.message for item in self.positions.values()]))
    @property
    def position_qifimsg(self):
        return dict(zip(self.positions.keys(), [item.qifimessage for item in self.positions.values()]))

    @property
    def position_profit(self):
        return sum([position.position_profit for position in self.positions.values()])

    @property
    def float_profit(self):
        return sum([position.float_profit for position in self.positions.values()])

    @property
    def frozen_margin(self):
        return sum([item.get('money') for item in self.frozen.values()])

    def transform_dt(self, times):
        if isinstance(times, str):

            if len(times) == 10:
                times = times+' 00:00:00'
            tradedt = datetime.datetime.strptime(times, '%Y-%m-%d %H:%M:%S') if len(
                times) == 19 else datetime.datetime.strptime(times.replace('_', '.'), '%Y-%m-%d %H:%M:%S.%f')
            return bson.int64.Int64(tradedt.timestamp()*1000000000)
        elif isinstance(times, datetime.datetime):
            return bson.int64.Int64(times.timestamp()*1000000000)


# 惰性计算


    @property
    def available(self):
        return self.money

    @property
    def margin(self):
        """保证金
        """
        return sum([position.margin for position in self.positions.values()])

    @property
    def commission(self):
        """本交易日内交纳的手续费
        """
        return sum([position.commission for position in self.positions.values()])

    @property
    def balance(self):
        """动态权益

        Arguments:
            self {[type]} -- [description]
        """

        return self.static_balance + self.deposit - self.withdraw + self.float_profit + self.close_profit

    def order_check(self, code: str, amount: int, price: float, towards: int, order_id: str) -> bool:
        """
        order_check是账户自身的逻辑, 你可以重写这个代码

        Attention: 需要注意的是 如果你修改了此部分代码 请注意如果你做了对于账户的资金的预操作请在结束的时候恢复

        :::如: 下单失败-> 请恢复账户的资金和仓位

        --> return  Bool
        """
        with self._lock:
            self._order_check_failure_reason = None
            res = False
            qapos = self.get_position(code)

            self.log(qapos.curpos)
            self.log(qapos.close_available)
            if towards in [
                ORDER_DIRECTION.BUY_CLOSE,
                ORDER_DIRECTION.BUY_CLOSETODAY,
                ORDER_DIRECTION.SELL_CLOSE,
                ORDER_DIRECTION.SELL_CLOSETODAY,
                ORDER_DIRECTION.SELL,
            ]:
                res = qapos.order_check(amount, price, towards, order_id)
                if not res:
                    self._order_check_failure_reason = 'position insufficient code={} amount={} towards={}'.format(
                        code, amount, towards
                    )
            elif towards in [ORDER_DIRECTION.BUY_OPEN,
                             ORDER_DIRECTION.SELL_OPEN,
                             ORDER_DIRECTION.BUY]:
                frozen_coeff = self._to_decimal(
                    self.market_preset.get_code(code).get(
                        "sell_frozen_coeff" if towards == ORDER_DIRECTION.SELL_OPEN else "buy_frozen_coeff",
                        1,
                    )
                )
                coeff = self._quantize_money(
                    self._to_decimal(price) *
                    self._to_decimal(self.market_preset.get_code(code).get("unit_table", 1)) *
                    frozen_coeff
                )
                moneyneed = self._quantize_money(coeff * self._to_decimal(amount))
                available = self._to_decimal(self.available)
                if available >= moneyneed + self._EPSILON:
                    self.money = self._decimal_to_float(available - moneyneed)
                    self.frozen[order_id] = {
                        'amount': amount,
                        'coeff': self._decimal_to_float(coeff),
                        'money': self._decimal_to_float(moneyneed)
                    }
                    res = True
                else:
                    self._order_check_failure_reason = 'insufficient funds available={} moneyneed={} code={} towards={}'.format(
                        self.available,
                        self._decimal_to_float(moneyneed),
                        code,
                        towards
                    )
                    self.log("开仓保证金不足 {}".format(self._order_check_failure_reason))
            return res

    def send_order(self, code: str, amount: float, price: float, towards: int, order_id: str = '', datetime: str = '') -> dict:
        with self._lock:
            if datetime:
                self.on_price_change(code, price, datetime)

            order_id = str(uuid.uuid4()) if order_id == '' else order_id
            if self.order_check(code, amount, price, towards, order_id):
                self.log("order check success")
                direction, offset = parse_orderdirection(towards)
                self.event_id += 1
                order = {
                    "account_cookie": self.user_id,
                    "user_id": self.user_id,
                    "instrument_id": code,
                    "towards": int(towards),
                    "exchange_id": self.market_preset.get_exchange(code),
                    "volume": int(amount),
                    "price": float(price),
                    "order_id": order_id,
                    "seqno": self.event_id,
                    "direction": direction,
                    "offset": offset,
                    "volume_orign": int(amount),
                    "price_type": "LIMIT",
                    "limit_price": float(price),
                    "time_condition": "GFD",
                    "volume_condition": "ANY",
                    "insert_date_time": self.transform_dt(self.dtstr),
                    'order_time': self.dtstr,
                    'create_time': time.time(),
                    "exchange_order_id": str(uuid.uuid4()),
                    "status": "ALIVE",
                    "volume_left": int(amount),
                    "last_msg": "已报"
                }
                self._mark_persist_pending()
                self.orders[order_id] = order
                self.log('下单成功 {}'.format(order_id))
                if self.model != 'BACKTEST':
                    self._sync_or_raise('send_order')
                self.on_ordersend(order)
                return order
            self.log("ORDER CHECK FALSE: {} {}".format(code, self._order_check_failure_reason or 'unknown reason'))
            return {
                'success': False,
                'reason': self._order_check_failure_reason or 'order_check_failed',
                'code': code,
                'amount': amount,
                'price': price,
                'towards': towards,
                'order_id': order_id,
            }

    def on_ordersend(self, order):
        pass

    def cancel_order(self, order_id):
        """Initial
        撤单/ 释放冻结/

        """
        with self._lock:
            od = self.orders.get(order_id)
            if od is None:
                self.log('cancel_order ignored missing order {}'.format(order_id))
                return False

            self._mark_persist_pending()
            remaining_volume = od.get('volume_left', 0)
            od['last_msg'] = '已撤单'
            od['status'] = "CANCEL"
            od['volume_left'] = 0

            if od['offset'] in ['CLOSE', 'CLOSETODAY']:
                pos = self.positions.get(od['exchange_id'] + '.' + od['instrument_id'])
                if pos is None:
                    self.log('cancel_order missing position for {}'.format(order_id))
                elif od['direction'] == 'BUY':
                    pos.volume_short_frozen_today = max(
                        0, pos.volume_short_frozen_today - remaining_volume
                    )
                else:
                    pos.volume_long_frozen_today = max(
                        0, pos.volume_long_frozen_today - remaining_volume
                    )
            else:
                self._release_open_order_frozen(order_id)

            self.orders[order_id] = od

            self.log('撤单成功 {}'.format(order_id))
            if self.model != 'BACKTEST':
                self._sync_or_raise('cancel_order')
            return True

    def make_deal(self, order: dict):
        if isinstance(order, dict):
            self.receive_deal(order["instrument_id"], trade_price=order["limit_price"], trade_time=self.dtstr,
                              trade_amount=order["volume_left"], trade_towards=order["towards"],
                              order_id=order['order_id'], trade_id=str(uuid.uuid4()))

    def receive_deal(self,
                     code,
                     trade_price,
                     trade_amount,
                     trade_towards,
                     trade_time,
                     message=None,
                     order_id=None,
                     trade_id=None,
                     realorder_id=None):
        with self._lock:
            if trade_id is not None and trade_id in self.trades:
                self.log('duplicate trade ignored {}'.format(trade_id))
                return self.trades[trade_id]

            if order_id in self.orders.keys():
                od = self.orders[order_id]
                frozen = self.frozen.get(
                    order_id, {'order_id': order_id, 'money': 0, 'price': 0, 'coeff': 1})
                vl = od.get('volume_left', 0)
                money_before = self.money
                if trade_amount == vl:
                    self.money = self._decimal_to_float(
                        self._to_decimal(self.money) + self._to_decimal(frozen['money'])
                    )
                    frozen['amount'] = 0
                    frozen['money'] = 0
                    od['last_msg'] = '全部成交'
                    od["status"] = "FINISHED"
                    od.pop('partial_fill_time', None)
                    self.log('全部成交 {}'.format(order_id))

                elif trade_amount < vl:
                    frozen['amount'] = vl - trade_amount
                    release_money = self._quantize_money(
                        self._to_decimal(trade_amount) * self._to_decimal(frozen.get('coeff', 1))
                    )
                    self.money = self._decimal_to_float(
                        self._to_decimal(self.money) + release_money
                    )
                    frozen['money'] = self._decimal_to_float(
                        self._to_decimal(frozen['money']) - release_money
                    )
                    od['last_msg'] = '部分成交'
                    od["status"] = "ALIVE"
                    od['partial_fill_time'] = time.time()
                    self.log('部分成交 {}'.format(order_id))

                od['volume_left'] -= trade_amount

                self.orders[order_id] = od
                self.frozen[order_id] = frozen
                self.event_id += 1
                trade_id = str(uuid.uuid4()) if trade_id is None else trade_id

                margin, close_profit, commission = self.get_position(code).update_pos(
                    trade_price, trade_amount, trade_towards)
                self.trades[trade_id] = {
                    "seqno": self.event_id,
                    "user_id":  self.user_id,
                    "trade_id": trade_id,
                    "exchange_id": od['exchange_id'],
                    "instrument_id": od['instrument_id'],
                    "order_id": order_id,
                    "exchange_trade_id": trade_id,
                    "direction": od['direction'],
                    "offset": od['offset'],
                    "volume": trade_amount,
                    "price": trade_price,
                    "trade_time": trade_time,
                    "commission": commission,
                    "trade_date_time": self.transform_dt(trade_time)}

                self._mark_persist_pending()
                self.money = self._decimal_to_float(
                    self._to_decimal(self.money) - (
                        self._to_decimal(margin) - self._to_decimal(close_profit)
                    )
                )
                self.close_profit = self._decimal_to_float(
                    self._to_decimal(self.close_profit) + (
                        self._to_decimal(close_profit) - self._to_decimal(commission)
                    )
                )

                self.log(
                    '成交审计 trade_id={} code={} price={} amount={} direction={} money_before={} money_after={} profit={}'.format(
                        trade_id,
                        code,
                        trade_price,
                        trade_amount,
                        trade_towards,
                        money_before,
                        self.money,
                        close_profit,
                    )
                )

                pos = self.get_position(code)
                if pos.volume_long == 0 and pos.volume_short == 0:
                    self.positions.pop(self.format_code(code))
                if self.model != "BACKTEST":
                    self._sync_or_raise('receive_deal')
                return self.trades[trade_id]
            return None

    def get_position(self, code: str = None) -> QA_Position:
        """
        兼容 code.XSHE 诸如

        """

        with self._lock:
            if code is None:
                return list(self.positions.values())[0]
            code = self.format_code(code)
            if code not in self.positions.keys():
                pos = QA_Position(
                    code=code,
                    account_cookie=self.user_id,
                    portfolio_cookie=self.portfolio,
                    username=self.username,
                )
                self.positions[code] = pos

            return self.positions[code]

    def expire_pending_orders(self, timeout_seconds=None, now=None):
        timeout = self._order_timeout_seconds if timeout_seconds is None else timeout_seconds
        current = time.time() if now is None else now
        expired = []
        for order_id, order in list(self.orders.items()):
            if order.get('status') == 'ALIVE' and order.get('volume_left', 0) > 0:
                create_time = order.get('create_time')
                if create_time is not None and current - create_time >= timeout:
                    if self.cancel_order(order_id):
                        expired.append(order_id)
        return expired

    def check_partial_deal_timeouts(self, timeout_seconds=None, now=None):
        timeout = self._partial_fill_timeout_seconds if timeout_seconds is None else timeout_seconds
        current = time.time() if now is None else now
        alerts = []
        for order_id, order in self.orders.items():
            partial_fill_time = order.get('partial_fill_time')
            if order.get('status') == 'ALIVE' and partial_fill_time is not None:
                if current - partial_fill_time >= timeout:
                    message = 'partial fill timeout order_id={} volume_left={}'.format(
                        order_id, order.get('volume_left')
                    )
                    self.log(message)
                    alerts.append(message)
        return alerts

    def run_background_tasks_once(self, now=None):
        partial_fill_alerts = self.check_partial_deal_timeouts(now=now)
        expired_orders = self.expire_pending_orders(now=now)
        return {
            'expired_orders': expired_orders,
            'partial_fill_alerts': partial_fill_alerts,
        }

    def query_trade(self):
        pass

    def on_tick(self, tick):
        pass

    def on_bar(self, bar):
        pass

    def format_code(self, code):

        if '.' in code:
            exchange_id, instrument_id = code.split('.', 1)
            if self.strict_code_format and self._is_unknown_derivative_code(
                instrument_id, exchange_id=exchange_id
            ):
                raise ValueError(
                    "unknown derivative instrument {!r} cannot use stock_cn fallback".format(
                        instrument_id
                    )
                )
            return code
        exchange_id = self.market_preset.get_exchange(code)
        if exchange_id is None:
            raise ValueError(
                "unknown instrument {!r} cannot infer exchange".format(code)
            )
        if self.strict_code_format and self._is_unknown_derivative_code(
            code, exchange_id=exchange_id
        ):
            raise ValueError(
                "unknown derivative instrument {!r} cannot use stock_cn fallback".format(
                    code
                )
            )
        return exchange_id + '.' + code

    def on_price_change(self, code, price, datetime=None):
        code = self.format_code(code)

        if code in self.positions.keys():
            try:
                pos = self.get_position(code.split('.')[1])
                if pos.last_price == price:
                    pass
                else:
                    pos.last_price = price

                if self.model != 'BACKTEST':
                    self._mark_persist_pending()
                    self._sync_or_raise('on_price_change')
            except Exception as e:

                self.log(e)

        if datetime:
            self.datetime = datetime

    def order_schedule(self, code: str, amount: float, price: float, towards: int, order_id: str = ''):
        """
        预调仓接口
        """
        if order_id == '':
            order_id = str(uuid.uuid4())
        orderx = {
            'code': code,
            'amount': amount,
            'price': price,
            'towards': towards,
            'order_id': order_id
        }
        self.schedule[order_id] = orderx


if __name__ == "__main__":
    # acc = QIFI_Account("x1", "x1")
    # acc.initial()

    # acc.log(acc.message)

    # r = acc.send_order('RB2001', 10, 5000, ORDER_DIRECTION.BUY_OPEN)
    # acc.log(r)

    # acc.receive_deal(r['instrument_id'], 4500, r['volume'], r['towards'],
    #                  acc.dtstr, order_id=r['order_id'], trade_id=str(uuid.uuid4()))

    # acc.log(acc.message)

    # acc.sync()

    # this is a stock account

    acc2 = QIFI_Account("x1", "x1")
    print('test for initial')
    acc2.initial()

    acc2.log(acc2.message)

    print('test for buy order')

    r = acc2.send_order('000001', 10, 12, ORDER_DIRECTION.BUY)
    acc2.log(r)

    print('test for receivedeal')

    acc2.receive_deal(r['instrument_id'], 11.8, r['volume'], r['towards'],
                      acc2.dtstr, order_id=r['order_id'], trade_id=str(uuid.uuid4()))

    acc2.log(acc2.message)

    print('test for sync')
    acc2.sync()

    print('test for settle')
