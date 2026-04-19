# QUANTAXIS 实盘交易系统任务书 v1.0

> **用途**：逐项追踪实盘交易各子系统的完成状态，额度用完时记录"暂停点"，下次恢复时从该条目继续。  
> **状态标记**：`[ ]` 待做 | `[~]` 进行中 | `[x]` 完成 | `[!]` 阻塞（需外部依赖）  
> **最后更新**：2026-04-16

---

## 现状摘要（2026-04-16 扫描结论）

| 模块 | 文件 | 状态 |
|---|---|---|
| 行情快照结构 `MDSnapshot` | `qadatastruct/mdsnapshot.rs` | ✅ 完整 |
| 行情分发 Actor `MarketDataDistributor` | `qamarket/qamdgateway/` | ✅ 完整 |
| WebSocket 行情服务 | `qamdgateway/ws_server.rs` | ✅ 完整 |
| 账户系统 `QA_Account` | `qaaccount/account.rs` | ✅ 完整 |
| QIFI 协议 | `qaprotocol/qifi/` | ✅ 完整 |
| 风控系统 5 阶段 | `qarisk/**` | ✅ 完整 |
| Redis 风控进程 | `qarisk/redis_process.rs` | ✅ 完整 |
| DAG/Cron 调度 | `qaexec/**` | ✅ 完整 |
| 回测框架 | `qastrategy/backtest.rs` | ✅ 基础版 |
| **实时行情数据源适配器** | `qareal/ctptrader.rs` 等 | ❌ 空文件 |
| **行情本地持久化管道** | — | ❌ 缺失 |
| **技术指标引擎（Rust）** | — | ❌ 缺失 |
| **实盘策略框架** | `qastrategy/qatemplate.rs` | ❌ on_bar_next 已注释 |
| **CTP/QMT 下单接口** | `qareal/ctptrader.rs` 等 | ❌ 空文件 |
| **套利交易框架** | — | ❌ 缺失 |
| **期权定价引擎** | — | ❌ 缺失 |
| **实盘主进程（Orchestrator）** | — | ❌ 缺失 |

---

## 第一部分：实时行情接入

### T-01 ── 腾讯/新浪 HTTP 行情适配器（A 股免费源）

**文件**：`qapro-rs/src/qamarket/qamdgateway/actors/qq_source.rs`  
**依赖**：`reqwest`（已在 Cargo.toml）  

- [ ] T-01-1 实现 `QQMarketSource` Actor（定时轮询腾讯 `hq.sinajs.cn` 接口）
  - 每 500ms 轮询一批股票代码
  - 解析 `var hq_str_sh600036="招商银行,..."` 格式字符串为 `MDSnapshot`
  - 向 `MarketDataDistributor` 发送 `MarketDataUpdate` 消息
- [ ] T-01-2 实现 `SinaMarketSource` Actor（备用源，格式相同）
- [ ] T-01-3 添加合约代码格式转换工具
  - `600036.SH` ↔ `sh600036`（腾讯/新浪）
  - `000001.SZ` ↔ `sz000001`
- [ ] T-01-4 编写单元测试（mock HTTP 响应，离线可运行）

---

### T-02 ── CTP 期货行情适配器

**文件**：`qapro-rs/src/qamarket/qareal/ctptrader.rs`  
**依赖**：`ctp-futures` crate 或 FFI 绑定  

- [ ] T-02-1 调研并选型 Rust CTP 绑定
  - 选项 A：`ctp-futures`（纯 Rust 封装）
  - 选项 B：`openctp-rs`（FFI，需 libthostmduserapi.so）
  - 选项 C：通过 Python pytdx/pyctp 桥接（先用，后迁移）
- [ ] T-02-2 实现 `CTPMdSource` Actor
  - `on_rtn_depth_market_data` → `MDSnapshot` 转换
  - 支持合约订阅/取消订阅
  - 断线自动重连（退避重试，最多 5 次，间隔 2^n 秒）
- [ ] T-02-3 处理 CTP 特有字段
  - `upper_limit_price`、`lower_limit_price` → `MDSnapshot.upper_limit/lower_limit`
  - `settlement_price`、`open_interest` 映射
- [ ] T-02-4 实现 `CTPTrader`（交易通道，详见 T-07）
- [ ] T-02-5 集成测试：连接 Simnow 模拟盘验证行情流

**暂停点**：若无法获取 CTP 账号，先完成 T-01 A 股部分，T-02 标记 `[!]`

---

### T-03 ── QMT/XTP 行情适配器（股票实盘）

**文件**：`qapro-rs/src/qamarket/qareal/qmttrader.rs`  

- [ ] T-03-1 实现基于 QMT Python API 的桥接器（先用 Python subprocess/TCP）
  - QMT → `xtquant.xtdata.subscribe_quote` → 推送到 Rust TCP 端口
  - Rust 侧 `QMTBridgeSource` Actor 读取 TCP 流解析 `MDSnapshot`
- [ ] T-03-2 后续升级：XTP C++ SDK FFI 直连
- [ ] T-03-3 编写桥接协议（JSON over TCP，保持和 WebSocket 消息格式一致）

---

### T-04 ── 行情数据源热切换与故障转移

**文件**：`qamarket/qamdgateway/actors/source_manager.rs`  

- [ ] T-04-1 实现 `SourceManager` Actor
  - 维护主/备数据源列表及当前活跃源
  - 监控心跳（每 5s 检查最后收到行情时间戳）
  - 超时 10s 自动切换到备用源并发出告警
- [ ] T-04-2 支持多源合并（去重 + 最新值优先）
- [ ] T-04-3 单元测试：mock 主源超时，验证切换逻辑

---

## 第二部分：行情本地持久化

### T-05 ── Tick 流实时写入 Parquet

**文件**：`qapro-rs/src/qastorage/tick_writer.rs`  
**依赖**：`polars`（已在 Cargo.toml）  

- [ ] T-05-1 实现 `TickWriter` 结构体
  - 内存缓冲区（按合约分桶，每桶最多 1000 条）
  - 触发条件：缓冲区满 OR 每 60s 定时刷写
  - 文件路径：`{data_dir}/{date}/{instrument_id}/tick_{yyyymmdd_hhmmss}.parquet`
- [ ] T-05-2 实现 Polars DataFrame 构建（`MDSnapshot` → `DataFrame`）
- [ ] T-05-3 实现 `TickWriterActor`（集成到 Actix 行情流）
  - 订阅 `MarketDataUpdate` 消息
  - 异步写盘，不阻塞行情分发
- [ ] T-05-4 文件索引（每日生成 `index.json` 记录各合约文件列表）
- [ ] T-05-5 单元测试：写入 100 条 tick，读取验证

---

### T-06 ── K 线合成与存储（1min/5min/日线）

**文件**：`qapro-rs/src/qastorage/bar_aggregator.rs`  

- [ ] T-06-1 实现 `BarAggregator` 结构体
  - 从 `MDSnapshot` 流合成 OHLCV K 线
  - 支持多周期：`1min`、`5min`、`15min`、`30min`、`60min`、`day`
  - 按交易所时区处理跨夜行情（夜盘合并至下一日）
- [ ] T-06-2 K 线写入 Parquet（路径：`{data_dir}/{date}/{instrument_id}/bar_{freq}.parquet`）
- [ ] T-06-3 K 线写入 MongoDB（可选，与 Python 侧共享）
  - 复用 `qaconnector/mongo/stock_day.rs` 写入接口
- [ ] T-06-4 K 线同步到已有 `QADataStruct_StockDay` / `QADataStruct_FutureMin`
- [ ] T-06-5 集成测试：回放历史 tick 验证 K 线合成正确性

---

### T-07（存储扩展） ── 历史数据补全工具

**文件**：`qapro-rs/src/qastorage/historical_fetcher.rs`  

- [ ] T-07-1 实现 `HistoricalFetcher`：从 MongoDB 读取已有日线/分钟线补充本地缓存
- [ ] T-07-2 实现开盘前自动检查并补全最近 N 天缺失数据

---

## 第三部分：交易信号计算

### T-08 ── 技术指标引擎（纯 Rust）

**文件**：`qapro-rs/src/qaindicator/mod.rs`  

- [ ] T-08-1 实现基础序列指标（增量计算，O(1) 更新）
  - `MA(n)` — 简单移动平均
  - `EMA(n)` — 指数移动平均
  - `MACD(fast, slow, signal)` — DIF/DEA/柱
  - `RSI(n)` — 相对强弱指标
  - `BOLL(n, k)` — 布林带（上中下轨）
  - `ATR(n)` — 平均真实波幅
  - `KDJ(n, m1, m2)` — 随机指标
  - `VOL_MA(n)` — 量均线
- [ ] T-08-2 实现 `IndicatorEngine` 结构体
  - 对每个合约维护独立的指标序列
  - `update(bar: BAR) -> IndicatorValues`（所有指标一次性输出）
- [ ] T-08-3 实现向量化批量计算（基于 Polars，用于回测）
- [ ] T-08-4 单元测试：对比已知数据集的指标结果

---

### T-09 ── 信号生成框架

**文件**：`qapro-rs/src/qasignal/mod.rs`  

- [ ] T-09-1 定义 `Signal` 类型
  ```rust
  pub struct Signal {
      pub instrument_id: String,
      pub direction: Direction,   // Buy/Sell
      pub signal_type: SignalType, // Open/Close/Hedge
      pub strength: f64,          // 0.0~1.0
      pub target_price: Option<f64>,
      pub stop_loss: Option<f64>,
      pub take_profit: Option<f64>,
      pub source: String,         // 策略标识
      pub timestamp_ms: i64,
  }
  ```
- [ ] T-09-2 定义 `SignalGenerator` trait
  ```rust
  pub trait SignalGenerator {
      fn name(&self) -> &str;
      fn on_bar(&mut self, bar: &BAR, indicators: &IndicatorValues) -> Vec<Signal>;
      fn on_tick(&mut self, snap: &MDSnapshot) -> Vec<Signal>;
  }
  ```
- [ ] T-09-3 实现 `SignalBus`（信号广播器）
  - 集中管理所有策略的输出信号
  - 信号优先级/过滤/去重
  - 持久化最近 N 条到 Redis（`signal:{account}:{instrument}`）
- [ ] T-09-4 实现内置参考策略
  - `MACrossStrategy`：MA 金死叉
  - `BreakoutStrategy`：突破策略（基于 BOLL/ATR）
  - `MeanReversionStrategy`：均值回归（RSI 超买超卖）

---

### T-10 ── 多品种信号聚合

**文件**：`qapro-rs/src/qasignal/aggregator.rs`  

- [ ] T-10-1 实现投票模型（多策略多品种投票聚合）
- [ ] T-10-2 实现信号强度归一化（避免大波动品种信号过强）
- [ ] T-10-3 实现信号冲突检测（同一品种同一时刻多空信号）

---

## 第四部分：实盘下单执行

### T-11 ── 实盘策略运行框架（LiveContext）

**文件**：`qapro-rs/src/qastrategy/livecontext.rs`  

- [ ] T-11-1 实现 `LiveContext`（扩展现有 `QAContext`）
  - 增加 `risk_service: RiskService` 字段
  - `send_order(signal: Signal)` → 先过风控 → 再发单
  - 支持多合约（现有 `QAContext` 单合约）
- [ ] T-11-2 实现 `LiveStrategyFunc` trait（扩展 `StrategyFunc`）
  - `on_tick(&MDSnapshot)` — Tick 级触发
  - `on_bar(&BAR, freq)` — K 线级触发
  - `on_order_ack(&OrderAck)` — 委托回报
  - `on_trade(&TradeReport)` — 成交回报
  - `on_risk_event(&RiskEvent)` — 风控事件
- [ ] T-11-3 填充 `QAStrategy::qatemplate.rs`（现已注释）为可运行示例
- [ ] T-11-4 单元测试（mock tick 流，验证信号→委托链路）

---

### T-12 ── CTP 下单接口实现

**文件**：`qapro-rs/src/qamarket/qareal/ctptrader.rs`  

- [ ] T-12-1 实现 `CTPTrader` 结构体
  - 登录/认证（brokerid, userid, password, appid, authcode）
  - `insert_order(order)` → `CThostFtdcInputOrderField`
  - `cancel_order(order_ref)` → `CThostFtdcInputOrderActionField`
  - 回调处理：`on_rtn_order`、`on_rtn_trade`、`on_err_insert_order`
- [ ] T-12-2 实现 `CTPBrokerAdapter`（实现 `qarisk::execution::BrokerAdapter` trait）
  - 接入风控 `OrderRouter`
- [ ] T-12-3 支持今仓/昨仓平仓（上期所特殊处理）
- [ ] T-12-4 委托状态机（Pending → PartialFilled → Filled / Cancelled / Rejected）
- [ ] T-12-5 连接 Simnow 模拟盘进行端到端测试

---

### T-13 ── QMT 股票下单接口（A 股实盘）

**文件**：`qapro-rs/src/qamarket/qareal/qmttrader.rs`  

- [ ] T-13-1 实现基于 QMT Python API 桥接的下单器
  - Rust → TCP → Python xtquant 桥接进程 → QMT 下单
  - 支持市价单/限价单
  - 回报通过 TCP 回传：委托确认、成交确认
- [ ] T-13-2 实现 `QMTBrokerAdapter`（实现 `BrokerAdapter` trait）
- [ ] T-13-3 实现涨跌停保护（下单前检查 `upper_limit/lower_limit`）

---

### T-14 ── 订单管理系统（OMS）完善

**文件**：`qapro-rs/src/qamarket/qaoms/mod.rs`（当前 `todo!()`）  

- [ ] T-14-1 实现 `QAOMS::add_main_account` / `add_sub_account`
- [ ] T-14-2 实现 `QAOMS::reload_account`（从 MongoDB 加载账户快照）
- [ ] T-14-3 实现委托队列管理（排队、超时撤单）
- [ ] T-14-4 实现持仓同步（CTP 持仓 ↔ QA_Account）
- [ ] T-14-5 实现账户状态持久化（每次成交后写入 MongoDB）

---

## 第五部分：套利交易框架

### T-15 ── 价差计算引擎

**文件**：`qapro-rs/src/qaarbitrage/spread.rs`  

- [ ] T-15-1 定义 `SpreadDefinition` 结构体
  ```rust
  pub struct SpreadDefinition {
      pub name: String,
      pub leg1: Leg,   // 主腿：品种、方向、乘数
      pub leg2: Leg,   // 辅腿
      pub spread_type: SpreadType, // Calendar/Inter-commodity/Basis/Options
  }
  ```
- [ ] T-15-2 实现实时价差计算（订阅两腿行情，实时更新价差序列）
- [ ] T-15-3 实现价差历史统计（均值、标准差、z-score、半衰期）
- [ ] T-15-4 内置常见价差模板
  - 期货跨期（rb2501-rb2505）
  - 跨品种（螺纹/热卷）
  - 期现（股指期货 IF 与 ETF 组合）
  - ETF 套利（ETF 净值 vs 市价）

---

### T-16 ── 统计套利信号

**文件**：`qapro-rs/src/qaarbitrage/stat_arb.rs`  

- [ ] T-16-1 实现 Kalman 过滤器（动态估计对冲比率）
- [ ] T-16-2 实现协整检验（ADF 检验，Rust 实现）
- [ ] T-16-3 实现均值回归信号生成
  - 开仓：z-score 超过 ±2σ
  - 平仓：z-score 回归到 ±0.5σ 以内
  - 止损：z-score 超过 ±3.5σ
- [ ] T-16-4 单元测试：用已知协整序列验证信号准确性

---

### T-17 ── 套利执行引擎（两腿同步下单）

**文件**：`qapro-rs/src/qaarbitrage/executor.rs`  

- [ ] T-17-1 实现 `ArbitrageExecutor` 结构体
  - 两腿同时下市价单（期货套利）
  - 处理一腿成交、一腿未成交的情况（自动对冲或撤单）
  - 超时保护（500ms 内未全成则撤单）
- [ ] T-17-2 实现腿间风险控制（单腿敞口限制）
- [ ] T-17-3 支持价差下单模式（主腿成交后挂辅腿限价单）

---

### T-18 ── ETF 套利（一篮子股票 ↔ ETF）

**文件**：`qapro-rs/src/qaarbitrage/etf_arb.rs`  

- [ ] T-18-1 实现 IOPV 实时计算（从成分股行情计算参考净值）
- [ ] T-18-2 实现 ETF 折溢价监控（`(市价 - IOPV) / IOPV`）
- [ ] T-18-3 实现申购套利（折价时买 ETF 赎回成分股）
- [ ] T-18-4 实现赎回套利（溢价时买成分股申购 ETF）
- [ ] T-18-5 处理成分股涨跌停限制（跳过无法交易的成分股）

---

## 第六部分：期权模块

### T-19 ── 期权定价引擎

**文件**：`qapro-rs/src/qaoption/pricing.rs`  

- [ ] T-19-1 实现 Black-Scholes 定价公式（欧式期权）
  ```rust
  pub fn bs_price(s: f64, k: f64, r: f64, t: f64, sigma: f64, is_call: bool) -> f64
  ```
- [ ] T-19-2 实现二叉树模型（美式期权，A 股个股期权）
- [ ] T-19-3 实现 Greeks 计算
  - Delta、Gamma、Theta、Vega、Rho
- [ ] T-19-4 实现隐含波动率求解（Newton-Raphson 迭代）
- [ ] T-19-5 单元测试：对比 Bloomberg/Python quantlib 基准值

---

### T-20 ── 期权策略模块

**文件**：`qapro-rs/src/qaoption/strategies.rs`  

- [ ] T-20-1 实现组合 Greeks 计算（投资组合级别 Delta/Gamma/Vega 汇总）
- [ ] T-20-2 实现波动率曲面构建（Skew + Term Structure）
- [ ] T-20-3 实现基础期权策略信号
  - 备兑开仓（Covered Call）
  - 保护性看跌（Protective Put）
  - 跨式策略（Straddle/Strangle）
  - 价差策略（Bull/Bear Spread）
- [ ] T-20-4 实现 Delta 中性对冲（Gamma Scalping）
  - 实时监控组合 Delta
  - Delta 超出阈值时自动下期货对冲单

---

### T-21 ── 期权风控扩展

**文件**：`qapro-rs/src/qarisk/option_rules.rs`  

- [ ] T-21-1 实现期权风控规则（扩展 `RuleEngine`）
  - `MaxGammaRule`：组合 Gamma 上限
  - `MaxVegaRule`：组合 Vega 上限
  - `DeltaNeutralRule`：Delta 偏离度告警
- [ ] T-21-2 集成到 `default_rule_engine(MarketType::CN)` 期权版本

---

## 第七部分：风控全程干预集成

### T-22 ── 风控与实盘交易主循环集成

**文件**：`qapro-rs/src/qaruntime/live_engine.rs`  

- [ ] T-22-1 实现 `LiveEngine` 主结构体（串联全部组件）
  ```
  LiveEngine {
      md_source: Box<dyn MarketDataSourceActor>,
      tick_writer: TickWriter,
      bar_aggregator: BarAggregator,
      indicator_engine: IndicatorEngine,
      signal_bus: SignalBus,
      risk_service: RiskService,
      redis_process: RiskRedisProcess,
      oms: QAOMS,
      broker: Box<dyn BrokerAdapter>,
  }
  ```
- [ ] T-22-2 实现主循环 `run()` 方法
  - Tick 到达 → 存储 → 合成 K 线 → 更新指标 → 生成信号
  - 信号 → 风控 evaluate() → 通过则下单 → OMS 管理委托
  - 成交回报 → 更新账户 → 更新风控状态 → 持久化
- [ ] T-22-3 实现优雅停机（Kill Switch → 撤销所有未成委托 → 持久化账户状态）
- [ ] T-22-4 实现日初初始化（加载前日持仓 → 风控日重置 → 订阅行情）
- [ ] T-22-5 实现日终结算（K 线刷写 → 账户日报 → 快照存 MongoDB）

---

### T-23 ── 风控监控 Dashboard（REST API）

**文件**：`qapro-rs/src/qahandlers/risk_handler.rs`  

- [ ] T-23-1 实现以下 REST 接口
  - `GET  /api/risk/state`          — 当前风控状态
  - `GET  /api/risk/events?n=100`   — 最近风控事件
  - `POST /api/risk/kill_switch`    — 远程触发 Kill Switch
  - `GET  /api/risk/portfolio`      — 当前持仓风险分解
  - `GET  /api/risk/forecast`       — EWMA 波动率预测
- [ ] T-23-2 集成到现有 `actix-web` 服务

---

### T-24 ── 风控参数热更新

**文件**：`qapro-rs/src/qarisk/config.rs`（扩展）  

- [ ] T-24-1 实现 `RiskConfig` 从 Redis 热读取（每分钟检查一次）
- [ ] T-24-2 支持参数变更不重启生效（`Arc<RwLock<RiskConfig>>`）
- [ ] T-24-3 实现参数变更审计日志（写入 Redis `risk:config_changes`）

---

## 第八部分：系统运维

### T-25 ── 配置系统重构

**文件**：`qapro-rs/src/qaenv/localenv.rs`（重构）  

- [ ] T-25-1 修复 `CONFIG` lazy_static 在测试环境调用 `process::exit(1)` 的问题
  - 方案：改为返回 `Result<Config, String>`，不调用 `process::exit`
  - 或：使用环境变量覆盖，不依赖 CLI args 解析
- [ ] T-25-2 支持从环境变量读取配置（`QUANTAXIS_CONFIG` 指定路径）
- [ ] T-25-3 实现配置验证（必填字段检查，连接测试）

---

### T-26 ── 启动脚本与进程管理

**文件**：`qapro-rs/scripts/`  

- [ ] T-26-1 编写 `run_live.sh`（启动实盘主进程 + 监控）
- [ ] T-26-2 编写 `run_md_gateway.sh`（行情网关独立启动）
- [ ] T-26-3 编写 systemd service 文件（自动重启、日志轮转）
- [ ] T-26-4 实现进程心跳监控（写 Redis `heartbeat:{process}:{ts}`）

---

### T-27 ── 日志与告警

**文件**：全局  

- [ ] T-27-1 统一日志格式（JSON structured logging，便于 ELK 采集）
- [ ] T-27-2 实现关键事件钉钉/微信通知（HTTP Webhook）
  - 风险等级升级告警
  - Kill Switch 触发
  - 大额成交通知
  - 连接断开告警
- [ ] T-27-3 实现每日绩效日报（自动计算日收益/最大回撤/夏普，发送到群）

---

### T-28 ── 回测与实盘对齐验证

**文件**：`qapro-rs/tests/backtest_live_parity.rs`  

- [ ] T-28-1 用相同历史数据运行回测框架和实盘框架
- [ ] T-28-2 验证信号完全一致（指标计算、风控逻辑）
- [ ] T-28-3 编写回归测试套件（防止实盘修改破坏回测结果）

---

## 依赖项检查清单

| Crate | 版本 | 用途 | 当前状态 |
|---|---|---|---|
| `reqwest` | 0.9.22 | HTTP 行情轮询 | ✅ 已有（需升级到 0.11+） |
| `redis` | 0.18.0 | Redis 风控进程 | ✅ 已有 |
| `polars` | 0.46 | DataFrame/Parquet | ✅ 已有 |
| `actix-web` | 4.0 | WebSocket/REST | ✅ 已有 |
| `ctp-futures` | — | CTP 期货接口 | ❌ 需添加 |
| `tokio` | — | 异步运行时 | ❌ 需添加（目前用 actix-rt） |
| 钉钉 SDK | — | 告警推送 | ❌ 用 reqwest HTTP 实现 |

---

## 执行优先级（推荐顺序）

```
P1（必须，支撑回测→实盘过渡）：
  T-01 → T-05 → T-06 → T-08 → T-09 → T-11

P2（A股实盘）：
  T-13 → T-14 → T-22 → T-25

P3（期货实盘）：
  T-02 → T-12 → T-22（期货扩展）

P4（套利与期权）：
  T-15 → T-16 → T-17 → T-19 → T-20

P5（运维）：
  T-23 → T-24 → T-26 → T-27 → T-28
```

---

## 暂停/恢复记录

| 日期 | 暂停于 | 已完成 | 备注 |
|---|---|---|---|
| 2026-04-16 | 任务书生成 | T-01~T-24 任务分解 | 开始执行 P1 |

---

*此任务书由 Claude Code 依据代码库深度扫描生成，每次恢复工作时请先运行 `cargo test --lib` 确认基线（当前：186 passed / 6 ignored）。*
