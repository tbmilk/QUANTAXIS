"""
qars_account_rs.py —— 基于 qapro_rs（本仓库 PyO3 绑定）的 QARSAccount 实现

当 qars3 不可用、但本仓库 qapro_rs 已编译时使用。
API 与 qars_account.py（基于 qars3）完全兼容。
"""

from typing import Dict, Optional
import pandas as pd
import qapro_rs as _rs


class QARSAccount:
    """
    QARS 高性能账户（基于 qapro_rs）

    与 qars3 版本的 QARSAccount API 完全兼容，可无缝替换。

    示例：
        >>> account = QARSAccount("my_account", init_cash=1_000_000)
        >>> account.buy("000001", 10.5, "2025-01-01", 100)
        >>> qifi = account.get_qifi()
        >>> print(qifi['accounts']['balance'])
    """

    def __init__(self,
                 account_cookie: str,
                 portfolio: str = "default",
                 init_cash: float = 1_000_000.0,
                 environment: str = "backtest"):
        self.account_cookie = account_cookie
        self.portfolio = portfolio
        self.init_cash = init_cash
        self.environment = environment
        self._account = _rs.PyQAAccount(
            account_cookie=account_cookie,
            portfolio_cookie=portfolio,
            init_cash=init_cash,
            environment=environment,
        )

    # ── 股票交易 ──────────────────────────────────────────────────────────────

    def buy(self, code: str, price: float, date: str, amount: int,
            validate: bool = True) -> bool:
        return self._account.buy(code, price, date, float(amount))

    def sell(self, code: str, price: float, date: str, amount: int,
             validate: bool = True) -> bool:
        return self._account.sell(code, price, date, float(amount))

    # ── 期货交易 ──────────────────────────────────────────────────────────────

    def buy_open(self, code: str, price: float, date: str, amount: int,
                 validate: bool = True) -> bool:
        return self._account.buy_open(code, price, date, float(amount))

    def sell_open(self, code: str, price: float, date: str, amount: int,
                  validate: bool = True) -> bool:
        return self._account.sell_open(code, price, date, float(amount))

    def buy_close(self, code: str, price: float, date: str, amount: int,
                  validate: bool = True) -> bool:
        return self._account.buy_close(code, price, date, float(amount))

    def sell_close(self, code: str, price: float, date: str, amount: int,
                   validate: bool = True) -> bool:
        return self._account.sell_close(code, price, date, float(amount))

    def buy_closetoday(self, code: str, price: float, date: str, amount: int,
                       validate: bool = True) -> bool:
        return self._account.buy_closetoday(code, price, date, float(amount))

    def sell_closetoday(self, code: str, price: float, date: str, amount: int,
                        validate: bool = True) -> bool:
        return self._account.sell_closetoday(code, price, date, float(amount))

    # ── 行情推送 ──────────────────────────────────────────────────────────────

    def on_price_change(self, code: str, price: float, datetime: str):
        self._account.on_price_change(code, price, datetime)

    # ── 结算 ──────────────────────────────────────────────────────────────────

    def settle(self, date: Optional[str] = None):
        if date is not None:
            try:
                self._account.settle(date)
                return
            except TypeError:
                pass
        self._account.settle()

    # ── 查询 ──────────────────────────────────────────────────────────────────

    def get_qifi(self) -> Dict:
        return self._account.get_qifi()

    def get_positions(self) -> pd.DataFrame:
        qifi = self.get_qifi()
        positions_dict = qifi.get('positions', {})
        if not positions_dict:
            return pd.DataFrame()
        positions_list = []
        for code, pos in positions_dict.items():
            row = dict(pos)
            row['code'] = code
            positions_list.append(row)
        return pd.DataFrame(positions_list)

    def get_account_info(self) -> Dict:
        return self._account.get_account_info()

    # ── 类方法 ────────────────────────────────────────────────────────────────

    @classmethod
    def from_qifi(cls, qifi_dict: Dict) -> 'QARSAccount':
        account_cookie = qifi_dict.get('account_cookie', 'unknown')
        portfolio      = qifi_dict.get('portfolio', 'default')
        init_cash      = qifi_dict.get('accounts', {}).get('pre_balance', 1_000_000.0)
        environment    = qifi_dict.get('environment', 'backtest')
        has_state = any(qifi_dict.get(key) for key in (
            'positions', 'orders', 'trades', 'events'
        ))
        if has_state:
            raise NotImplementedError(
                "QARSAccount.from_qifi() 目前无法恢复持仓/订单/成交等完整状态，"
                "拒绝静默导入以避免账户状态丢失。"
            )
        return cls(account_cookie=account_cookie, portfolio=portfolio,
                   init_cash=init_cash, environment=environment)

    # ── 上下文管理器 ──────────────────────────────────────────────────────────

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.settle()

    def __repr__(self) -> str:
        info = self.get_account_info()
        return (
            f"QARSAccount(cookie='{self.account_cookie}', "
            f"balance={info.get('balance', 0):.2f}, "
            f"backend='qapro_rs')"
        )
