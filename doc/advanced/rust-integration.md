# qapro-rs：仓库内 Rust 运行时

**更新日期**: 2026-04-17

本项目的 Rust 组件位于仓库根目录 `qapro-rs/`，是 Python `QUANTAXIS` 的扩展运行时，而**不是**独立的 PyO3 加速库。两者的定位区别：

| | Python `QUANTAXIS` | `qapro-rs` |
|---|---|---|
| 定位 | 主框架，生产就绪 | Rust 运行时与连接器，实验/扩展 |
| 数据落库 | `quantaxis save stock_day` 等 CLI | 只读为主；可通过 qaconnector 写账户状态 |
| 策略生态 | 完整（CTA/套利/期权） | 骨架，部分实现 |
| 行情接入 | Python TDX/CTP/BaoStock | qamdgateway（WebSocket 分发） |
| 账户协议 | Python QIFI_Account | QATrader（QIFI over WebSocket） |

> 若只是想加速 Python 数据处理，可用 `pip install -e .[performance]` 引入 Polars/orjson；若想使用 Rust 运行时功能，见下文。

---

## 模块概览

| 模块 | 关键文件 | 功能 |
|---|---|---|
| `qadatastruct` | `stockday.rs`、`futureday.rs`、`mdsnapshot.rs` | Polars DataFrame 包装；MDSnapshot L1/L2 快照（含10档盘口） |
| `qaprotocol/qifi` | `account.rs`、`order.rs`、`position.rs` | QIFI 账户协议，与 Python 完全兼容，可序列化到 MongoDB |
| `qaprotocol/qamd` | `mod.rs`（重导出 mdsnapshot） | 标准化行情数据协议 |
| `qaconnector/mongo` | `mongoclient.rs`、`stock_day.rs` | 读写 Python 写入的 MongoDB 日线/账户集合 |
| `qaconnector/clickhouse` | `ckclient.rs` | 查询 ClickHouse 历史 K 线（股票/期货） |
| `qaindicator` | `indicators/`（19个） | 流式技术指标（SMA/EMA/MACD/RSI/BOLL/ATR 等），O(1) 增量计算 |
| `qafactor/operators` | `ring_buffer.rs`、`rolling.rs`、`welford.rs` | 流式因子算子（RingBuffer/Welford/滑动统计） |
| `qarisk/riskmodes` | `blacklitterman.rs`、`mvo.rs`、`cov.rs` | 投资组合优化（Black-Litterman/均值方差/协方差） |
| `qatrader` | `qatrader.rs`、`msg.rs` | QATrader：QIFI WebSocket 实盘连接器，含 MongoDB 状态同步 |
| `qamarket/qamdgateway` | `actors/`、`ws_server.rs` | Actix Actor 行情分发网关 + WebSocket 推送服务 |
| `qamarket/qaoms` | `mod.rs` | 订单管理系统（QAOMS） |
| `qaexec` | `qadag/`、`qacron/`、`qaschedule/` | DAG 有向无环图任务调度 + Cron 定时 |
| `parsers` | `sql/`、`value/pql_value.rs` | SQL-like 表达式解析引擎（nom 6.1.2）  |

---

## 构建

工具链已锁定（`qapro-rs/rust-toolchain.toml` → stable），无需手动安装特定版本。

```bash
# 从仓库根目录或 qapro-rs/ 目录均可
cargo build --release

# 编译检查（比 build 快）
cargo check --package qapro-rs

# 同时编译示例（确认 API 没有漂移）
cargo build --examples --release
```

配置文件需单独提供（复制 `qapro-rs/example.toml` 并修改）：

```bash
cargo run --release -- example.toml
```

---

## 与 Python 共用数据

Python 侧用 `quantaxis save stock_day` 写入 MongoDB 后，Rust 可直接读取：

```rust
use qapro_rs::qaconnector::mongo::stock_day::{
    load_stock_day_for_backtest, fetch_stock_codes_from_list,
};

// 读取单只股票日线（返回 QADataStruct_StockDay，Polars DataFrame 包装）
let data = load_stock_day_for_backtest(
    &mongo_client, "000001.SZ", "2023-01-01", "2024-01-01"
).await?;
```

完整示例见 `qapro-rs/examples/backtest_mongo.rs`：

```bash
cargo run --example backtest_mongo --release -- example.toml
```

`example.toml` 中的 `[hisdata]` 需指向 Python 写入数据的同一个 MongoDB `quantaxis` 库。

---

## 行情网关（qamdgateway）

接收任意行情源推送的 `MDSnapshot`，通过 WebSocket 分发给客户端。

```rust
use qapro_rs::qamarket::qamdgateway::{
    actors::MarketDataDistributor,
    actors::messages::{MarketDataUpdate, MarketDataSource},
};
use actix::Actor;

let distributor = MarketDataDistributor::new().start();

// 任意来源的行情快照都可以发送到分发器
distributor.do_send(MarketDataUpdate(snapshot, MarketDataSource::Custom));
```

启动 WebSocket 服务端（客户端连接 `ws://host:8080/ws`）：

```rust
HttpServer::new(move || {
    App::new()
        .app_data(web::Data::new(distributor.clone()))
        .route("/ws", web::get().to(ws_handler))
})
.bind("0.0.0.0:8080")?.run().await?;
```

客户端订阅行情：

```json
{"aid": "subscribe_quote", "ins_list": "SHFE.rb2501,SSE.688286"}
```

---

## 流式技术指标（qaindicator）

指标均实现 `Next<f64>` trait，逐 K 线增量更新，O(1) 时间复杂度：

```rust
use qapro_rs::qaindicator::{ema::Ema, macd::Macd, rsi::Rsi};

let mut ema20 = Ema::new(20).unwrap();
let mut macd = Macd::new(12, 26, 9).unwrap();

for bar in bars {
    let ema_val = ema20.next(bar.close);
    let macd_val = macd.next(bar.close);
    // macd_val.macd / .signal / .histogram
}
```

支持的指标：SMA、EMA、MACD、RSI、BollingerBands、ATR、TrueRange、OBV、ROC、MFI、FastStochastic、SlowStochastic、EfficiencyRatio、HHV、LLV、Max、Min、StdDev、MovingAverage。

---

## 实盘交易（QATrader）

通过 QIFI WebSocket 协议与 Python 侧的 QAServer 通信：

```rust
use qapro_rs::qatrader::qatrader::QATrader;

let mut trader = QATrader::new(
    "account_001".to_string(),
    "password".to_string(),
    "ws://localhost:8010/".to_string(),
    1_000_000.0, // 初始资金
    "SIM".to_string(),
);

// 处理 WebSocket 消息
trader.parse(ws_message);

// 同步账户状态到 MongoDB
trader.sync(); // 或 trader.sync_async().await
```

---

## 投资组合优化（qarisk）

```rust
use qapro_rs::qarisk::riskmodes::{
    mvo::MvoOptimizer,
    blacklitterman::BlackLittermanModel,
    cov::CovarianceMatrix,
};

// 均值方差优化
let optimizer = MvoOptimizer::new(returns_matrix, risk_aversion);
let weights = optimizer.optimize()?;

// Black-Litterman
let bl = BlackLittermanModel::new(market_weights, cov_matrix, views, confidences);
let posterior = bl.posterior_returns()?;
```

---

## 依赖版本注意事项

以下版本已固定，**不要随意升级**：

- `polars = "0.46"` — `diff()` 是独立函数，非 Series 方法；Rolling 相关结构用 `RollingOptionsFixedWindow`
- `mongodb = "1.1.1"` sync 模式，bson `0.13.0`（旧版 `doc!` 宏 API）
- `actix = "0.12"` + `actix-web = "4.0.0-beta.5"`（beta 组合，不能升）

---

## Python 侧使用（qapro_rs / QARSBridge）

`maturin develop --features pyo3-bindings` 编译完成后，即可在 Python 中直接使用：

```python
# 方式一：通过 QARSBridge（推荐，有 Python fallback）
from QUANTAXIS.QARSBridge import ma, ema, macd, rsi, boll, atr, kdj, QADataFrame

close = df['close'].tolist()

ma20   = ma(close, 20)                     # list[float]
ema12  = ema(close, 12)
macd_r = macd(close)                       # dict: macd/signal/histogram
rsi14  = rsi(close, 14)
boll_r = boll(close)                       # dict: upper/mid/lower
atr14  = atr(df['high'].tolist(), df['low'].tolist(), close, 14)
kdj_r  = kdj(df['high'].tolist(), df['low'].tolist(), close)  # dict: k/d/j

# 方式二：QADataFrame —— 贴近 qars2.QADataFrame
qadf = QADataFrame(df)                     # pandas DataFrame 或 dict
qadf.ma(5)                                 # MA5
qadf.macd()                                # MACD
qadf.boll(20)                              # 布林带
qadf.atr(14)                               # ATR
qadf.to_dataframe()                        # 转回 pandas DataFrame

# 方式三：多标的并行计算（Rust Rayon，自动多线程）
from QUANTAXIS.QARSBridge import parallel_apply

close_arrays = [df1['close'].tolist(), df2['close'].tolist(), ...]
ma20_all = parallel_apply(close_arrays, 'ma', period=20)   # list of list[float]

# 方式四：PyArrow 互操作
import pyarrow as pa
from QUANTAXIS.QARSBridge import from_arrow, process_arrow

arrow_table = pa.Table.from_pandas(df)
qadf = from_arrow(arrow_table)             # → QADataFrame
result = process_arrow(arrow_table,        # → dict(原始列 + 指标列)
                       ops=['ma5', 'ma20', 'ema12', 'macd', 'rsi14'])
```

> 若 `qapro_rs` 未编译，`QARSBridge` 自动降级到等价的纯 Python 实现，API 完全相同。

---

## 相关文档

- [qapro-rs 与 Python 详细对照](./qapro-rs-status.md)
- [实盘交易系统任务书](./live_trading_taskbook.md)
- [数据桥接](./data-bridge.md)
- [性能优化](./performance-tuning.md)
