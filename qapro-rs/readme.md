## QAPRO-RS（QUANTAXIS 仓库内 Rust 组件）

> 与文档中的 **QARS2** / `pip install quantaxis[rust]` 是不同概念：后者为独立 PyO3 加速库。本目录是**运行时 + 连接器 + 行情网关 + 策略骨架**，见 [qapro-rs 与 Python 对照说明](../doc/advanced/qapro-rs-status.md)。

### 模块概览

| 模块 | 路径 | 功能 |
|---|---|---|
| `qadatastruct` | `src/qadatastruct/` | 日线/分钟线/期货日线/期货分钟线/L1/L2 快照/因子等 Polars 包装 |
| `qadatastruct::mdsnapshot` | `src/qadatastruct/mdsnapshot.rs` | **MDSnapshot** L1/L2 行情快照（10档盘口，期货扩展字段） |
| `qaprotocol::qifi` | `src/qaprotocol/qifi/` | QIFI 账户协议（Account/Order/Position/Execution） |
| `qaprotocol::qamd` | `src/qaprotocol/qamd/` | 标准化行情数据协议（重导出 MDSnapshot） |
| `qaconnector` | `src/qaconnector/` | ClickHouse / MongoDB / Redis / RabbitMQ 连接器 |
| `qaindicator` | `src/qaindicator/` | **流式技术指标**（SMA/EMA/MACD/RSI/BOLL/ATR 等 19 个，O(1) 增量） |
| `qafactor` | `src/qafactor/` | 因子回测（Polars）+ 流式因子算子（RingBuffer/Welford/rolling） |
| `qarisk` | `src/qarisk/` | 5 阶段风控 + MVO/Black-Litterman/协方差优化 |
| `qatrader` | `src/qatrader/` | **QATrader**：QIFI WebSocket 实盘连接器，含 MongoDB 状态同步 |
| `qamarket::qamdgateway` | `src/qamarket/qamdgateway/` | **行情网关**：Actix Actor 分发器 + WebSocket 服务端 |
| `qamarket::qaoms` | `src/qamarket/qaoms/` | 订单管理系统（QAOMS） |
| `qaexec` | `src/qaexec/` | DAG 有向无环图任务调度 + Cron 定时 |
| `qastrategy` | `src/qastrategy/` | 策略回测框架骨架 |
| `parsers` | `src/parsers/` | SQL-like 表达式解析引擎（nom 6.1.2） |

### 依赖服务（按需）

ClickHouse、Redis、MongoDB、RabbitMQ。若无本地服务，可用 `database.yaml` 通过 Docker 拉起（按需删减服务块）。

### 编译

```bash
# 从仓库根目录或 qapro-rs/ 目录均可
cargo build --release

# 编译检查（更快）
cargo check --package qapro-rs

# 建议同时编译示例，避免 API 漂移未被发现
cargo build --examples --release
```

### 运行

必须传入 **TOML 配置文件** 路径：

```bash
cargo run --release -- example.toml
```

将 `example.toml` 中的 `[DataPath].cache` 改为本机可写目录。

### MongoDB 日线（与 Python QUANTAXIS 共用库）

Python 执行 `quantaxis save stock_day` 后，Rust 可直接读取并转为 `QADataStruct_StockDay`：

```rust
use qapro_rs::qaconnector::mongo::stock_day::load_stock_day_for_backtest;

let data = load_stock_day_for_backtest(
    &client, "000001.SZ", "2023-01-01", "2024-01-01"
).await?;
```

示例：`cargo run --example backtest_mongo --release -- example.toml`

### 行情网关（qamdgateway）

从 `qautlra-rs/qamd-rs` 整合的市场数据网关，提供：

- 标准化 `MDSnapshot`（L1/L2 深度，支持股票/期货/期权/ETF）
- 基于 Actix Actor 的 `MarketDataDistributor`（增量推送，减少带宽）
- `WsSession`（WebSocket 服务端，支持 TradingView 格式和传统格式）

**接入自定义数据源：**

```rust
use qapro_rs::qamarket::qamdgateway::{
    actors::MarketDataDistributor,
    actors::messages::{MarketDataUpdate, MarketDataSource},
};
use actix::Actor;

let distributor = MarketDataDistributor::new().start();
distributor.do_send(MarketDataUpdate(snapshot, MarketDataSource::Custom));
```

**启动 WebSocket 服务端：**

```rust
HttpServer::new(move || {
    App::new()
        .app_data(web::Data::new(distributor.clone()))
        .route("/ws", web::get().to(ws_handler))
})
.bind("0.0.0.0:8080")?.run().await?;
```

客户端订阅：`{"aid": "subscribe_quote", "ins_list": "SHFE.rb2501,SSE.688286"}`

### 流式技术指标（qaindicator）

来自 `qaaccount-rs`，实现 `Next<f64>` trait，O(1) 逐 K 线更新：

```rust
use qapro_rs::qaindicator::{ema::Ema, macd::Macd};

let mut ema20 = Ema::new(20).unwrap();
let val = ema20.next(close_price);
```

### 实盘交易（QATrader）

通过 QIFI WebSocket 协议与 Python QAServer 通信，含 MongoDB 账户状态同步：

```rust
use qapro_rs::qatrader::qatrader::QATrader;

let mut trader = QATrader::new("account_001".into(), "password".into(),
    "ws://localhost:8010/".into(), 1_000_000.0, "SIM".into());
trader.parse(ws_message);  // 处理 WebSocket 消息
trader.sync();             // 同步状态到 MongoDB
```

### 版本约束（不要升级以下依赖）

- **Polars 0.46**：`diff` 使用 `polars::prelude::diff(&series, n, NullBehavior)`；`RollingOptions` → `RollingOptionsFixedWindow`
- **mongodb 1.1.1**：bson 0.13.0 旧版 API，不兼容新版
- **actix 0.12 + actix-web 4.0.0-beta.5**：beta 组合固定，不能升

### SIMD / nightly

若需 SIMD 相关实验，可尝试 `cargo +nightly build --release`；当前 `rust-toolchain.toml` 默认 **stable**。

@yutiansut
