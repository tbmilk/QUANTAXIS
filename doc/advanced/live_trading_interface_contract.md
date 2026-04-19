# 实盘统一接口约定

更新时间: 2026-04-18
适用范围: `qapro-rs` 第一阶段实盘主链路

## 1. 目标

本文件用于固化第一阶段的统一接口，避免:

- 先写死期货实现，再回头重构
- QMT 接入时新增第二套主流程
- MongoDB 回放和实时接入走两套逻辑

第一阶段统一要求:

- `CTP(openctp)` 和 `QMT bridge` 只在“行情入口”和“交易出口”不同
- `MongoDB` 回放与实时行情共享同一消费接口
- `LiveEngine` 只消费通用事件，不感知具体柜台
- `CTP(openctp)` 的实现优先基于 `ctp2rs`，但仍需通过仓库内统一接口承载

## 2. 主接口

### `MarketDataSource`

职责:

- 统一实时行情源与历史回放源
- 输出标准化 `MDSnapshot`

最小能力:

- `name()`
- `source_type()`
- `health_check()`
- `subscribe()`
- `unsubscribe()`
- `next_event()` 或等价事件推送

实现方:

- `CTPMdSource`
- `QMTBridgeSource`
- `MongoReplaySource`

### `BrokerAdapter`

职责:

- 统一下单、撤单、查询、健康检查

最小能力:

- `submit_order()`
- `cancel_order()`
- `query_position()`
- `query_account_state()`
- `query_open_orders()`
- `health_check()`

实现方:

- `CTPTrader`
- `QMTBrokerAdapter`

### `OmsService`

职责:

- 统一订单生命周期管理
- 幂等处理回报
- 账户/持仓同步

最小能力:

- `record_submit()`
- `apply_order_ack()`
- `apply_trade_report()`
- `reload_state()`
- `snapshot()`

### `SignalGenerator`

职责:

- 接收 Tick/Bar/回报事件并产生信号

最小能力:

- `on_snapshot()`
- `on_bar()`
- `on_order_ack()`
- `on_trade_report()`

### `LiveContext`

职责:

- 串起 `MarketDataSource`、`RiskService`、`OrderRouter`、`OmsService`
- 给策略提供统一运行上下文

最小能力:

- `evaluate_and_submit()`
- `handle_order_ack()`
- `handle_trade_report()`
- `portfolio_snapshot()`
- `market_state()`

## 3. 统一事件模型

第一阶段统一事件流:

1. `MarketDataEvent`
2. `Signal`
3. `OrderSnapshot`
4. `OrderAck`
5. `TradeReport`
6. `RiskDecision`

推荐主循环:

```text
MarketDataSource
  -> SignalGenerator
  -> RiskService
  -> OrderRouter
  -> BrokerAdapter
  -> OmsService
  -> PortfolioSnapshot / MongoDB
```

## 4. 不允许的实现方式

- 不允许在 `LiveEngine` 内写死 CTP API
- 不允许 QMT 单独复制一套 OMS 或风控主流程
- 不允许 MongoDB 回放绕过 `MarketDataSource`

## 5. 第一阶段验收要求

- `CTP(openctp)` 必须跑在这套接口上
- `QMT bridge` 必须跑在这套接口上
- `MongoReplaySource` 必须跑在这套接口上
