# AGENTS.md

本文件用于约束 Codex 在本仓库中的默认工作方式，目标是减少重复扫描和无效 token 消耗。

## 首选上下文

进入仓库后，默认按下面顺序读取，除非用户任务明显需要深入源码:

1. `doc/codex/project_static_overview.md`
2. `doc/codex/project_dynamic_snapshot.json`
3. `doc/codex/rust_architecture_overview.md`
4. `README.md`
5. `doc/README.md`

如果上面四个文件已经足够回答问题或制定改动方案，不要再全量扫描仓库。

## 扫描原则

- 禁止默认全仓扫描。
- 只按用户任务进入相关模块。
- 优先扫描 `QUANTAXIS/` 下与任务直接相关的子模块。
- 优先打开入口文件、`__init__.py`、模块 `readme.md`、调用链起点。

## 默认跳过目录

除非任务明确要求，否则不要优先扫描这些目录:

- `venv/`
- `target/`
- `.git/`
- `test/qaself/`
- `docker/`
- `.pytest_cache/`
- `.ipynb_checkpoints/`

## 模块选择速查

- 安装/启动/CLI: `setup.py`, `QUANTAXIS/__main__.py`, `QUANTAXIS/QACmd`
- 数据抓取: `QUANTAXIS/QAFetch`
- 数据入库/同步: `QUANTAXIS/QASU`
- 数据结构: `QUANTAXIS/QAData`
- 指标/因子: `QUANTAXIS/QAIndicator`, `QUANTAXIS/QAFactor`
- 账户/订单/回测: `QUANTAXIS/QIFI`, `QUANTAXIS/QAMarket`, `QUANTAXIS/QARSBridge`
- 异步/调度/服务: `QUANTAXIS/QAEngine`, `QUANTAXIS/QASchedule`, `QUANTAXIS/QAWebServer`, `QUANTAXIS/QAPubSub`
- 通用工具: `QUANTAXIS/QAUtil`

## 输出要求

- 面向初学者说明时，优先用“数据层/分析层/交易层/基础设施层”的结构解释。
- 说明调用关系时，先讲入口，再讲中间数据结构，再讲执行与服务化。
- 如果需要更新仓库记忆，请优先更新 `doc/codex/` 下的索引文件，而不是新增零散说明。

## 文档与产物存放

- 生成的、需要随项目共同打包发布的项目文档，默认存入 `doc/` 下的合适位置。
- 生成的任务书、阶段性执行记录、一般性日志结果，默认存入 `test/codex/` 下。
