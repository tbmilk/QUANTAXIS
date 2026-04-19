# qapro-rs 与 Python QUANTAXIS 对照说明

**更新日期**：2026-04-17

---

## 1. 三个名称请分清

| 名称 | 含义 |
|------|------|
| **Python `QUANTAXIS`** | 主框架：行情抓取、Mongo 存储、CLI、回测与策略等，生产就绪。 |
| **本仓库 `qapro-rs/`** | 仓库内的 **Rust 运行时 + 连接器 + 行情网关 + 策略骨架**，与 Python 框架互补。 |
| **文档中的 QARS2 / `pip install quantaxis[rust]`** | 独立的 PyO3 加速库（轮子），**与本目录 `qapro-rs` 不是同一个东西**。 |

完整「Rust 加速」安装请以 `setup.py` / PyPI 说明为准；本文聚焦 `qapro-rs/` 目录本身。

---

## 2. 当前模块状态

### 已完整实现

| 模块 | 文件 | 说明 |
|---|---|---|
| 行情快照 | `qadatastruct/mdsnapshot.rs` | MDSnapshot L1/L2，10档盘口，期货扩展字段，OptionalF64/I64 |
| 行情分发网关 | `qamarket/qamdgateway/` | Actix Actor 分发器 + WebSocket 服务端 |
| QIFI 账户协议 | `qaprotocol/qifi/` | Account/Order/Position/Execution，可序列化到 MongoDB |
| 行情协议类型 | `qaprotocol/qamd/` | 重导出 MDSnapshot，含 Tick/Daily/Minute 类型 |
| 数据结构 | `qadatastruct/` | 日线/分钟线/期货日线/期货分钟线/复权因子/板块/L1/L2 快照 |
| MongoDB 连接 | `qaconnector/mongo/` | 读取 Python 写入的 stock_day；保存/读取 QIFI 账户状态 |
| ClickHouse 连接 | `qaconnector/clickhouse/ckclient.rs` | get_stock/get_future/get_stock_adj/get_factor |
| 流式技术指标 | `qaindicator/` | 19个指标（SMA/EMA/MACD/RSI/BOLL/ATR 等），O(1) 增量计算 |
| 流式因子算子 | `qafactor/operators/` | RingBuffer/Welford/滑动统计，IncrementalOperator trait |
| 因子回测 | `qafactor/factorbacktest.rs` | 基于 Polars 的因子分层回测 |
| 投资组合优化 | `qarisk/riskmodes/` | Black-Litterman / 均值方差优化 / 协方差 / Ledoit-Wolf 压缩 |
| 风控系统 | `qarisk/` | 5 阶段风控（budget/forecast/market/rules/statemachine） |
| 实盘交易连接器 | `qatrader/` | QATrader：QIFI WebSocket 协议，含 MongoDB 同步 |
| 订单管理 | `qamarket/qaoms/` | QAOMS，含 add_main/sub_account，reload_account |
| DAG/Cron 调度 | `qaexec/` | 有向无环图任务调度 + Cron 定时 |
| SQL 解析器 | `parsers/` | SQL-like 表达式（nom 6.1.2），支持 Upper/Lower/Ceil/Floor/Round |
| 账户系统 | `qaaccount/` | 账户状态机 |
| 策略框架骨架 | `qastrategy/` | 基础回测框架 |

### 空占位（骨架未实现）

| 文件 | 待实现内容 |
|---|---|
| `qamarket/qareal/ctptrader.rs` | CTP 下单接口（依赖 CTP .so 文件） |
| `qamarket/qareal/qmttrader.rs` | QMT 股票下单（需 XTP SDK） |
| `qamarket/qasim/` | 模拟交易撮合引擎 |
| `qastrategy/qatemplate.rs` | 实盘策略模板（on_bar_next 已注释） |

---

## 3. 与 Python 侧功能对比

### Python 侧已有、Rust 侧已对标

- QIFI 账户协议（双侧均完整）
- MongoDB 日线读取（`stock_day` 集合格式一致）
- 技术指标（Python QAIndicator ↔ Rust qaindicator）

### Python 侧已有、Rust 侧部分实现

- 回测框架（Rust 有骨架，Python 更完整）
- 实盘交易（Rust QATrader 支持 QIFI WebSocket，Python 支持 CTP/OES 等多柜台）

### Python 侧已有、Rust 侧未对标

- TDX / `save stock_*` 全链路 CLI（数据落库仍建议用 Python）
- Mongo 全集合读写与 `QASU` 保存任务
- BaoStock / TuShare / PyTDX 行情抓取
- 与 Python 策略生态 1:1 兼容的 API

**结论：生产环境数据落库与更新仍建议以 `quantaxis save ...` 为主；Rust 侧适合作为扩展运行时、高性能本地计算或行情分发网关。**

---

## 4. 构建与运行

```bash
# 编译（从仓库根目录或 qapro-rs/ 均可）
cargo build --release

# 检查（更快）
cargo check --package qapro-rs

# 运行（必须提供 TOML 配置）
cargo run --release -- example.toml
```

配置文件关键字段（复制 `qapro-rs/example.toml` 修改）：

```toml
[hisdata]
uri = "mongodb://127.0.0.1:27017"
db = "quantaxis"          # 与 Python QUANTAXIS 同库

[DataPath]
cache = "/tmp/qapro/"     # 改为本机可写目录
```

依赖服务（按需）：MongoDB、Redis、ClickHouse、RabbitMQ。可通过 `qapro-rs/database.yaml` 用 Docker 拉起。

---

## 5. 已知版本约束

| 依赖 | 版本 | 约束原因 |
|---|---|---|
| polars | 0.46 | `diff()` 已迁至独立函数；`RollingOptionsFixedWindow` 命名 |
| mongodb | 1.1.1 | bson 0.13.0 旧版 API，新版 bson 不兼容 |
| actix | 0.12 + actix-web 4.0.0-beta.5 | beta 组合固定，不能升至 stable |
| nom | 6.1.2 | SQL 解析器全部基于此版本 |

---

## 6. 若要进一步与 Python 对齐

1. **契约先行**：为 MongoDB 集合名与字段生成共享 schema（JSON Schema / protobuf），Rust 与 Python 共同遵守。
2. **数据路径**：Rust 只读 Python 已写入的 Parquet/MongoDB，或共用 ClickHouse 表结构。
3. **CLI**：用 `clap` 提供 `save` 子命令子集，内部规则与 `QASU` 保持一致（或 HTTP 调 Python 服务）。
4. **PyO3 发布**：若需 `pip` 一体化，单独做 PyO3 crate，与本目录 `qapro-rs` 解耦或 workspace 聚合。

---

[返回高级功能](../README.md) | [Rust 集成概述](./rust-integration.md) | [实盘任务书](./live_trading_taskbook.md)
