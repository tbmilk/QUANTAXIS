"""
QARSBridge/redis_consumer.py - Rust 风控事件 Redis 消费者

消费 Rust `RiskRedisProcess` 发布到以下频道的事件：
  - risk:decisions  → 风控决策（放行/拒绝）
  - risk:alerts     → 风控告警（超阈值、止损触发）

用法::

    from QUANTAXIS.QARSBridge.redis_consumer import RiskEventConsumer

    consumer = RiskEventConsumer("redis://127.0.0.1:6379/")

    @consumer.on_decision
    def handle_decision(event):
        if event.get("approved"):
            print("风控放行:", event["order_id"])

    consumer.start()   # 后台线程，非阻塞
    # ...
    consumer.stop()
"""

from __future__ import annotations

import json
import logging
import threading
import time
from typing import Callable, Dict, List, Optional

logger = logging.getLogger(__name__)

# ─── 频道常量（与 Rust RiskRedisProcess 保持一致） ───────────────────────────
CHANNEL_DECISIONS = "risk:decisions"
CHANNEL_ALERTS    = "risk:alerts"
CHANNEL_CTRL      = "risk:ctrl"


class RiskEventConsumer:
    """
    订阅 Rust 风控进程发布的 Redis 事件。

    参数
    ----
    redis_url : str
        Redis 连接 URL，格式 ``redis://[password@]host[:port][/db]``。
    channels : list[str], optional
        要订阅的频道列表，默认 ``["risk:decisions", "risk:alerts"]``。
    decode_responses : bool
        是否自动将字节解码为字符串（默认 True）。
    """

    def __init__(
        self,
        redis_url: str = "redis://127.0.0.1:6379/",
        channels: Optional[List[str]] = None,
        decode_responses: bool = True,
        reconnect_interval: float = 1.0,
        max_reconnect_attempts: Optional[int] = None,
        quiet_errors: bool = True,
        suppress_callback_traceback: Optional[bool] = None,
    ) -> None:
        self._redis_url = redis_url
        self._channels = channels or [CHANNEL_DECISIONS, CHANNEL_ALERTS]
        self._decode_responses = decode_responses
        self._reconnect_interval = reconnect_interval
        self._max_reconnect_attempts = max_reconnect_attempts
        self._quiet_errors = quiet_errors
        self._suppress_callback_traceback = (
            quiet_errors
            if suppress_callback_traceback is None
            else suppress_callback_traceback
        )

        self._decision_callbacks: List[Callable[[Dict], None]] = []
        self._alert_callbacks:    List[Callable[[Dict], None]] = []
        self._raw_callbacks:      List[Callable[[str, Dict], None]] = []

        self._thread: Optional[threading.Thread] = None
        self._stop_event = threading.Event()
        self._pubsub = None  # redis.client.PubSub
        self.last_error: Optional[str] = None
        self.connection_attempts = 0
        self.message_count = 0
        self.malformed_message_count = 0
        self.callback_error_count = 0

    # ─── 回调注册（支持装饰器） ───────────────────────────────────────────────

    def on_decision(self, func: Callable[[Dict], None]) -> Callable[[Dict], None]:
        """注册风控决策回调（channel = risk:decisions）。可用作装饰器。"""
        self._decision_callbacks.append(func)
        return func

    def on_alert(self, func: Callable[[Dict], None]) -> Callable[[Dict], None]:
        """注册风控告警回调（channel = risk:alerts）。可用作装饰器。"""
        self._alert_callbacks.append(func)
        return func

    def on_message(self, func: Callable[[str, Dict], None]) -> Callable[[str, Dict], None]:
        """注册原始消息回调，参数为 (channel, payload_dict)。"""
        self._raw_callbacks.append(func)
        return func

    # ─── 生命周期 ────────────────────────────────────────────────────────────

    def start(self) -> None:
        """在后台线程中启动 Pub/Sub 消费循环（非阻塞）。"""
        if self._thread and self._thread.is_alive():
            logger.warning("RiskEventConsumer 已在运行，忽略重复 start()")
            return
        self._stop_event.clear()
        self._thread = threading.Thread(
            target=self._consume_loop,
            name="RiskEventConsumer",
            daemon=True,
        )
        self._thread.start()
        logger.info("RiskEventConsumer 已启动，订阅频道: %s", self._channels)

    def stop(self) -> None:
        """通知消费线程退出，并等待其结束（最多 2 秒）。"""
        self._stop_event.set()
        self._close_pubsub()
        if self._thread:
            self._thread.join(timeout=2)
        logger.info("RiskEventConsumer 已停止")

    def is_running(self) -> bool:
        """返回消费线程是否正在运行。"""
        return bool(self._thread and self._thread.is_alive())

    # ─── 阻塞模式（不开后台线程） ─────────────────────────────────────────────

    def run_forever(self) -> None:
        """在当前线程中阻塞运行（适合作为独立进程使用）。"""
        self._stop_event.clear()
        self._consume_loop()

    # ─── 内部实现 ─────────────────────────────────────────────────────────────

    def _close_pubsub(self) -> None:
        if self._pubsub:
            try:
                self._pubsub.unsubscribe()
                self._pubsub.close()
            except Exception as exc:
                self.last_error = str(exc)
                self._log_error("关闭 Redis pubsub 失败", exc)
            finally:
                self._pubsub = None

    def _log_error(self, message: str, exc: Exception, include_traceback: Optional[bool] = None) -> None:
        if include_traceback is None:
            include_traceback = not self._quiet_errors
        if include_traceback:
            logger.exception("%s: %s", message, exc)
        else:
            logger.warning("%s: %s", message, exc)

    def _consume_loop(self) -> None:
        try:
            import redis as redis_lib
        except ImportError:
            logger.error(
                "缺少 redis 包。请执行: pip install redis"
            )
            return

        while not self._stop_event.is_set():
            try:
                self.connection_attempts += 1
                client = redis_lib.from_url(
                    self._redis_url,
                    decode_responses=self._decode_responses,
                )

                self._pubsub = client.pubsub(ignore_subscribe_messages=True)
                self._pubsub.subscribe(*self._channels)
                self.last_error = None
                logger.info("已连接 Redis %s，等待消息…", self._redis_url)

                for raw_msg in self._pubsub.listen():
                    if self._stop_event.is_set():
                        break
                    if raw_msg is None or raw_msg.get("type") != "message":
                        continue
                    channel: str = raw_msg.get("channel", "")
                    data_str: str = raw_msg.get("data", "{}")

                    try:
                        payload: Dict = json.loads(data_str)
                    except json.JSONDecodeError:
                        self.malformed_message_count += 1
                        logger.warning("无法解析 JSON，channel=%s, data=%r", channel, data_str)
                        continue

                    self.message_count += 1
                    self._dispatch(channel, payload)
            except Exception as exc:
                self.last_error = str(exc)
                self._log_error("Redis 消费循环异常", exc)
                if self._stop_event.is_set():
                    break
                if (
                    self._max_reconnect_attempts is not None
                    and self.connection_attempts >= self._max_reconnect_attempts
                ):
                    logger.error(
                        "Redis 重连次数达到上限 %s，停止消费循环",
                        self._max_reconnect_attempts,
                    )
                    self._stop_event.set()
                    break
                if self._reconnect_interval > 0:
                    time.sleep(self._reconnect_interval)
            finally:
                self._close_pubsub()

    def _dispatch(self, channel: str, payload: Dict) -> None:
        # 原始回调（所有频道）
        for cb in self._raw_callbacks:
            self._safe_call(cb, channel, payload)

        if channel == CHANNEL_DECISIONS:
            for cb in self._decision_callbacks:
                self._safe_call(cb, payload)
        elif channel == CHANNEL_ALERTS:
            for cb in self._alert_callbacks:
                self._safe_call(cb, payload)

    def _safe_call(self, func: Callable, *args) -> None:
        try:
            func(*args)
        except Exception as exc:
            self.callback_error_count += 1
            self._log_error(
                "回调 %s 异常" % getattr(func, "__name__", repr(func)),
                exc,
                include_traceback=not self._suppress_callback_traceback,
            )
