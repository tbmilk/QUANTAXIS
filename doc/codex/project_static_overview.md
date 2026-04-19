# QUANTAXIS 项目静态总览

更新时间: 2026-04-18
适用版本: QUANTAXIS `2.1.0.alpha2`

## 1. 项目是什么

QUANTAXIS 是一个面向量化研究、数据获取、账户建模、回测与实盘扩展的 Python 框架。

它当前的主线能力可以概括为四层:

1. 数据层: `QAFetch` + `QASU` + `QAData`
2. 分析层: `QAIndicator` + `QAFactor` + `QAAnalysis`
3. 交易层: `QIFI` + `QAMarket` + `QARSBridge`
4. 基础设施层: `QAEngine` + `QASchedule` + `QAPubSub` + `QAWebServer`

同时，这个仓库不是“只有 Python”。它还包含一条明确的 Rust 实现线:

1. Python 包内的 `QARSBridge`，负责把 Rust 高性能能力桥接到 Python
2. 仓库内独立 Rust 子项目 `qapro-rs/`，负责运行时、连接器、行情网关、指标、因子、风控和策略骨架

2.1 版本的明显变化是:

- Python 版本提升到 `3.9+`
- 增加 `QARSBridge`，为 Rust 核心提供 Python 桥接
- 增加 `QADataBridge`，强调跨语言零拷贝数据交换
- 依赖整体现代化，围绕 `pandas 2.x`、`pyarrow`、新版本 `pymongo/redis/tornado`

## 2. 先看哪里

初次理解本项目，优先按下面顺序阅读:

1. `README.md`
2. `setup.py`
3. `QUANTAXIS/__init__.py`
4. `doc/README.md`
5. `doc/codex/rust_architecture_overview.md`
6. 按模块定向看 `QUANTAXIS/<模块>/readme.md` 或核心入口文件

如果只是为了回答“这个仓库是什么、怎么启动、模块怎么分工”，通常不需要全量扫描源码。

## 3. 关键入口

### 包入口

- `QUANTAXIS/__init__.py`
  - 暴露大量公共 API
  - 聚合数据、指标、账户、抓取、调度、Web 等能力
- `QUANTAXIS/__main__.py`
  - `python -m QUANTAXIS` 时进入 `QA_cmd()`

### CLI 入口

`setup.py` 中定义的 console scripts:

- `quantaxis=QUANTAXIS.QACmd:QA_cmd`
- `quantaxisq=QUANTAXIS.QAFetch.QATdx_adv:bat`
- `qarun=QUANTAXIS.QACmd.runner:run`
- `qawebserver=QUANTAXIS.QAWebServer.server:main`

这说明命令行和运行时的主入口集中在 `QACmd`、`QAFetch`、`QAWebServer`。

## 4. 核心模块分工

### `QAFetch`

职责: 从多种市场或数据源抓取行情、列表、实时数据、交易日等。

特征:

- 文件数量多，属于外部接口密集区
- 包含 `QAQuery`、`QAQuery_Advance`、`Fetcher`、各数据源适配器
- 是数据流的前端入口

### `QASU`

职责: 数据保存、更新、同步、入库。

常见用途:

- 把抓取的数据保存到 MongoDB / ClickHouse 等存储
- 维护历史行情、账户、回测结果、策略数据

### `QAData`

职责: 项目内部的数据结构层。

重点:

- 提供 DataStruct 系列对象
- 负责复权、重采样、转换、筛选、查询、展示
- 连接“原始数据”和“策略/分析逻辑”

### `QAIndicator`

职责: 技术指标与指标计算。

用途:

- 为策略或研究提供指标函数
- 是因子构建的基础部件之一

### `QAFactor`

职责: 因子研究与因子管理。

重点:

- `feature` 表示原始特征/因子计算结果
- `featureAnalysis` 做标准化、中性化等二次处理
- `featureView` 做因子集合管理

### `QAAnalysis`

职责: 面向分析任务的辅助模块。

通常位于指标/因子之上，做更具体的分析输出。

### `QIFI`

职责: 统一账户协议与账户管理。

重点:

- 为多市场交易提供一致账户抽象
- 适合承载持仓、现金、订单、成交等状态

### `QAMarket`

职责: 市场侧抽象，如订单、仓位、市场预设。

更接近交易语义和撮合前后的账户变化逻辑。

### `QARSBridge`

职责: Python 到 Rust 核心的桥接层。

意义:

- 2.1 版本的重要新增能力
- 提供高性能账户和回测能力
- 未安装 Rust 组件时允许回退到 Python 实现

### `QADataBridge`

职责: 不同数据表示或进程之间的高性能交换。

重点:

- Pandas / Arrow / Polars 转换
- 共享内存传输
- 为跨语言和高吞吐场景准备

### `QAEngine`

职责: 事件、任务、线程、异步执行框架。

内部角色:

- `QA_Worker`: 干活的对象
- `QA_Task`: 某时刻要做的任务
- `QA_Thread` / `QA_Engine`: 调度与执行载体

### `QAStrategy`

职责: 策略基类和策略工具。

适合作为“如何把账户、行情、执行框架连起来”的观察点。

### `QASchedule`

职责: 调度任务。

用于周期性任务或系统化运行编排。

### `QAPubSub`

职责: 发布/订阅消息机制。

适合跨模块事件传播、异步消息消费。

### `QAWebServer`

职责: Web 服务入口。

用于把框架能力暴露为服务接口。

### `QAUtil`

职责: 通用工具库。

范围很广，包括:

- 日期和交易日工具
- 日志
- 配置
- 编码/文本/文件工具
- MongoDB/SQL/缓存辅助

它是底层公共依赖，改动时影响面通常较大。

### `QASetting`

职责: 本地配置、执行环境、缓存与本地化。

### `QACmd`

职责: CLI 命令入口。

如果要理解“用户怎么启动 QUANTAXIS”，先看这里。

## 5. 仓库其他重要区域

- `doc/`: 主文档中心，适合初学者
- `qabook/`: LaTeX/PDF 技术手册
- `examples/`: 示例
- `test/`: 测试
- `docker/`: 部署与镜像
- `qapro-rs/`: 仓库内独立 Rust 子项目
- `scripts/`: 辅助脚本

## 6. Rust 部分怎么理解

### `QARSBridge` 和 `qapro-rs` 的区别

这是理解仓库 Rust 能力时最容易混淆的地方。

- `QARSBridge`
  - 位置: `QUANTAXIS/QARSBridge`
  - 作用: Python 侧桥接层
  - 面向对象: Python 用户
  - 主要价值: 让 Python 代码直接使用 Rust 账户/回测能力，并在缺少 Rust 组件时回退到 Python 实现

- `qapro-rs`
  - 位置: `qapro-rs/`
  - 作用: 仓库内独立 Rust 工程
  - 面向对象: Rust 开发者、系统开发者、服务化场景
  - 主要价值: 提供独立的 Rust 运行时、连接器、行情网关、流式指标、因子/风控/策略基础设施

可以把它们理解为:

- `QARSBridge` 是“Python 调 Rust”
- `qapro-rs` 是“Rust 自己就能跑一整套子系统”

### `qapro-rs` 里有什么

根据 `qapro-rs/readme.md` 和 `qapro-rs/src/lib.rs`，这个 Rust 子项目至少包含下面几类能力:

- `qaprotocol`
  - 定义协议层，尤其是 `QIFI` 账户协议和行情协议
- `qaconnector`
  - 连接 ClickHouse、MongoDB、Redis、RabbitMQ
- `qamarket`
  - 包含行情网关 `qamdgateway` 和订单管理相关能力
- `qaindicator`
  - 流式技术指标，强调逐条更新和 O(1) 增量计算
- `qafactor`
  - 因子回测与因子计算
- `qarisk`
  - 风控和组合优化
- `qaexec`
  - DAG 调度与定时执行
- `qatrader`
  - 基于 QIFI WebSocket 的交易连接器
- `qastrategy`
  - 策略骨架
- `qadatastruct`
  - Rust 侧数据结构，和多市场行情/因子数据相关
- `qapyo3`
  - 可选 PyO3 绑定，用于把 Rust 能力暴露给 Python

### Rust 在本项目里的功能定位

Rust 不是简单做“性能优化插件”，它在这个仓库里承担的是系统级能力:

1. 高性能账户和回测
2. 高吞吐数据结构与流式指标
3. 多数据源连接器
4. 行情网关和 WebSocket 服务
5. 风控、因子、策略运行时
6. 通过 PyO3 或桥接层回流给 Python 使用

### 为什么项目要保留 Python + Rust 双线

对初学者来说，可以这样理解:

- Python 线适合研究、脚本开发、生态集成、快速验证
- Rust 线适合性能敏感、服务化、实时系统、长期演进
- `QIFI` 协议负责把不同语言实现统一起来

所以这个项目不是“Python 调几个 Rust 扩展”这么简单，而是在尝试做跨语言的一致量化基础设施。

## 7. 给初学者的功能理解

可以把 QUANTAXIS 想成一条流水线:

1. `QAFetch` 从外部拿数据
2. `QASU` 把数据存起来、更新起来
3. `QAData` 把数据包装成统一结构
4. `QAIndicator` / `QAFactor` 在数据上做研究
5. `QIFI` / `QAMarket` 表示账户、订单、持仓
6. `QAStrategy` / `QAEngine` 把策略真正跑起来
7. `QAWebServer` / `QAPubSub` / `QASchedule` 负责系统化运行
8. `QARSBridge` / `QADataBridge` 提供 2.1 的高性能扩展能力

如果把 Rust 也放进这条图里，那么可以再补一层:

9. `qapro-rs` 在 Python 框架旁边提供一套 Rust 侧运行时与系统组件
10. `QIFI` 协议把 Python / Rust / C++ 的账户表示尽量统一
11. `QARSBridge` 把 Rust 的高性能账户/回测能力接回 Python 侧

## 8. 后续阅读策略

按需求选模块，不要默认全仓扫描:

- 看“怎么装、怎么启动”: `README.md`、`setup.py`、`QACmd`
- 看“怎么取数”: `QAFetch`、`QASU`
- 看“怎么做数据结构”: `QAData`
- 看“怎么做指标/因子”: `QAIndicator`、`QAFactor`
- 看“怎么做账户/回测”: `QIFI`、`QAMarket`、`QARSBridge`
- 看“怎么服务化”: `QAWebServer`、`QAPubSub`、`QASchedule`
- 看“Rust 在做什么”: `qapro-rs/readme.md`、`qapro-rs/src/lib.rs`、`QUANTAXIS/QARSBridge`
