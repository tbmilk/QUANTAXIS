# QUANTAXIS Rust 架构总览

更新时间: 2026-04-18
适用范围: `qapro-rs/`、`QUANTAXIS/QARSBridge`、`QUANTAXIS/QIFI`

## 1. 先说结论

本仓库中的 Rust 能力由两部分组成:

1. `qapro-rs/`
   - 仓库内独立 Rust 子项目
   - 提供运行时、连接器、行情网关、指标、因子、风控、调度、交易连接器、策略骨架
2. `QUANTAXIS/QARSBridge`
   - Python 侧桥接层
   - 负责把 Rust 高性能能力接回 Python 使用

它们的共同协议基础是 `QIFI`。

所以，理解本项目里的 Rust，不要只盯着“性能优化”四个字，而要把它看成:

- 一套 Rust 侧量化基础设施
- 一层 Python ↔ Rust 桥接能力
- 一份跨语言统一账户/交易协议

## 2. 三个名字必须分清

### `qapro-rs`

这是仓库内的 Rust 工程。

它更接近“系统实现”:

- 运行时
- 连接器
- 行情网关
- 风控
- 因子
- 调度
- 策略骨架

### `QARSBridge`

这是 Python 包内的桥接层。

它更接近“Python 用户入口”:

- Python 代码不必直接理解 Rust crate 内部细节
- 优先使用桥接 API
- 缺少 Rust 组件时，可以回退到 Python 实现

### `QIFI`

这是账户和交易相关的统一协议。

它更接近“契约”:

- 统一账户结构
- 统一持仓、订单、成交表示
- 支持 Python / Rust / C++ 多语言一致

## 3. Rust 在整体架构中的位置

把整个仓库简化后，可以理解成下面这张图:

```text
外部数据源/数据库/消息系统
        |
        v
  qapro-rs.qaconnector
        |
        v
  qapro-rs.qadatastruct / qaprotocol
        |
        +----------------------+
        |                      |
        v                      v
 qaindicator/qafactor      qamarket/qatrader
 qarisk/qaexec             qamdgateway/qaoms
        |                      |
        +----------+-----------+
                   |
                   v
             QIFI 协议层
                   |
        +----------+-----------+
        |                      |
        v                      v
   Python QARSBridge       Python QIFI/QAMarket
        |                      |
        +----------+-----------+
                   |
                   v
             Python QUANTAXIS
```

简单说:

- `qapro-rs` 负责 Rust 侧的“干活”
- `QIFI` 负责跨语言统一语义
- `QARSBridge` 负责把 Rust 能力暴露给 Python

## 4. `qapro-rs` 的模块地图

下面按“系统职责”来理解，而不是按目录名字死记。

### 4.1 协议与数据结构

#### `qaprotocol`

职责:

- 定义账户协议
- 定义行情协议
- 约束跨模块、跨语言的数据交换格式

重点:

- `qaprotocol::qifi`
  - 账户、订单、持仓、成交等核心结构
- `qaprotocol::qamd`
  - 标准化行情数据协议

这是 Rust 侧最重要的“契约层”。

#### `qadatastruct`

职责:

- 承载行情、K 线、因子、L1/L2 快照等数据结构
- 与 `polars` 结合，提供高性能数据表示

重点:

- `mdsnapshot.rs`
  - L1/L2 行情快照
  - 对行情网关尤其关键

### 4.2 数据接入与外部系统连接

#### `qaconnector`

职责:

- 连接 MongoDB、ClickHouse、Redis、RabbitMQ
- 为 Rust 运行时提供统一的数据入口和状态存取

实际作用:

- 读取 Python 已经落库的数据
- 读取或保存账户状态
- 接入实时消息系统

对理解项目非常关键的一点是:

`qapro-rs` 并不是完全脱离 Python 数据体系，它经常是复用 Python QUANTAXIS 已经存好的数据。

### 4.3 计算与研究能力

#### `qaindicator`

职责:

- 流式技术指标计算
- O(1) 增量更新

适用场景:

- 实时逐 bar 计算
- 高频或低延迟场景

#### `qafactor`

职责:

- 因子算子
- 因子回测
- 流式统计

重点:

- `RingBuffer`
- `Welford`
- rolling 统计

这是 Rust 侧因子计算能力的核心。

#### `qarisk`

职责:

- 风控
- 投资组合优化

内容:

- 均值方差优化
- Black-Litterman
- 协方差估计
- 多阶段风控

### 4.4 交易与市场侧系统

#### `qatrader`

职责:

- QIFI WebSocket 交易连接器
- 与 Python 侧服务通信
- 同步账户状态

适合把它理解为:

- Rust 侧交易客户端
- 不是单纯的账户对象

#### `qamarket`

职责:

- 行情网关
- 订单管理

重点子模块:

- `qamdgateway`
  - 行情分发
  - WebSocket 服务端
  - 面向实时行情系统
- `qaoms`
  - 订单管理系统

### 4.5 运行时与调度

#### `qaexec`

职责:

- DAG 调度
- Cron 定时执行

作用:

- 让 Rust 侧不只是函数库，而是可以组织任务执行

#### `qastrategy`

职责:

- 策略骨架
- 回测/执行框架的最小结构

现状:

- 有基础能力
- 但和 Python 主框架相比，生态完整度仍偏弱

### 4.6 Python 绑定

#### `qapyo3`

职责:

- 可选的 PyO3 绑定
- 把 Rust crate 暴露成 Python 模块

这部分不是默认主路径，但它说明了一件重要事情:

Rust 并不是只能通过进程外服务接入 Python，也可以直接通过 Python 扩展模块接入。

## 5. `QARSBridge` 在里面扮演什么角色

对 Python 用户来说，不建议先读完整个 `qapro-rs`。

更实用的入口是 `QARSBridge`。

它的角色是:

1. 隐藏 Rust 内部实现细节
2. 提供更贴近 Python 使用习惯的 API
3. 尽量保证“Rust 可用时加速，Rust 不可用时也能跑”

所以:

- 做 Python 应用开发，优先从 `QARSBridge` 理解
- 做系统开发或性能开发，再深入 `qapro-rs`

## 6. `QIFI` 为什么是核心

如果没有 `QIFI`，那么 Python 和 Rust 两边会各自维护一套账户语义，很快就会失控。

`QIFI` 的意义就在于:

1. 统一账户结构
2. 统一持仓结构
3. 统一订单和成交结构
4. 支持增量同步
5. 适合 JSON / MongoDB / WebSocket 传输

这意味着:

- Python 账户对象和 Rust 账户对象不是“看起来差不多”
- 而是应该围绕同一协议做实现

这也是本项目能做跨语言协同的关键。

## 7. Rust 与 Python 的边界

初学者最容易误解的是“Rust 会不会取代 Python 主框架”。

就当前仓库形态看，答案是否定的。

更准确的理解是:

- Python 主框架仍是主入口
- Rust 是高性能扩展和系统能力补充
- 两者不是简单替代关系，而是分工关系

当前更合理的边界是:

- Python 更适合:
  - CLI
  - 数据抓取
  - 数据落库
  - 研究脚本
  - 生态整合
- Rust 更适合:
  - 流式计算
  - 实时行情分发
  - 高频指标/因子计算
  - 运行时和服务化
  - 协议一致性和性能敏感组件

## 8. 从哪里开始读

### 如果你是 Python 使用者

按下面顺序:

1. `README.md`
2. `doc/codex/project_static_overview.md`
3. `QUANTAXIS/QARSBridge`
4. `QUANTAXIS/QIFI`
5. `QUANTAXIS/QAMarket`

目标:

- 先知道 Rust 给 Python 带来了什么
- 再看账户协议和交易语义

### 如果你是想读 Rust 系统设计

按下面顺序:

1. `qapro-rs/readme.md`
2. `qapro-rs/src/lib.rs`
3. `qapro-rs/Cargo.toml`
4. `QUANTAXIS/QARSBridge/QIFI_PROTOCOL.md`
5. `qapro-rs/src/main.rs`
6. 再按模块深入 `qaconnector`、`qaprotocol`、`qamarket`、`qaindicator`

目标:

- 先理解 crate 边界
- 再理解协议层
- 最后才进入模块实现

### 如果你是要改性能相关代码

优先看:

1. `qaindicator`
2. `qafactor`
3. `qadatastruct`
4. `qapyo3`

### 如果你是要改交易/账户相关代码

优先看:

1. `QIFI_PROTOCOL.md`
2. `qaprotocol::qifi`
3. `qatrader`
4. `QARSBridge`
5. `QIFI` / `QAMarket`

## 9. 当前成熟度判断

按仓库当前材料，Rust 部分更适合这样判断:

- 已有清晰架构主线
- 已有真实模块实现，不是空壳
- 已能承担部分运行时和高性能任务
- 但整体生态成熟度仍低于 Python 主框架

因此最稳妥的认知是:

- Python 是主线生产框架
- Rust 是增强线和系统线

## 10. 阅读时的注意事项

### 不要混淆三件事

1. `qapro-rs`
2. `QARSBridge`
3. 文档里提到的独立 Rust/PyO3 加速库

它们有关联，但不是同一个目录、同一个交付物。

### 不要默认认为 Rust 负责取数

当前数据抓取和落库主线仍主要在 Python。

Rust 更常见的角色是:

- 读取已存在的数据
- 高性能计算
- 运行时服务

### 不要先从 `main.rs` 深挖业务细节

`main.rs` 更像运行时拼装入口。

真正决定长期结构的是:

- 协议层
- 数据结构层
- 连接器层
- 市场/交易层

## 11. 给初学者的一句话解释

如果你只想用一句话记住本仓库的 Rust 部分，可以记成:

“`qapro-rs` 是 QUANTAXIS 的 Rust 运行时和系统组件，`QARSBridge` 是 Python 使用这些能力的桥，`QIFI` 是它们共享的账户协议。” 

