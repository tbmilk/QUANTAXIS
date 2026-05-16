# QUANTAXIS Python 3.14 迁移记录

更新时间: 2026-05-16
适用版本: QUANTAXIS `2.1.0a2`
目标解释器: Python `3.14.4`

## 1. 迁移目标

本次迁移的直接目标不是“理论声明支持 Python 3.14”，而是满足以下实际条件:

1. QUANTAXIS 主包可在 Python 3.14 下安装并导入
2. `QATdx` / `pytdx` / `opentdx` 路径可在 Python 3.14 下工作
3. 常用核心模块可通过 smoke import
4. `_api` 级别导入不因历史可选依赖阻塞
5. 可选分析栈在 Python 3.14 下尽量升级到可用状态

迁移优先级曾设定为:

1. 优先验证 Python 3.14
2. 若 3.14 不可行，再回退验证 Python 3.12

最终结果是: **Python 3.14 迁移可行，未再回退到 3.12。**

## 2. 迁移结论

截至 2026-05-16，QUANTAXIS 已完成到 Python 3.14 的主流程迁移，`venv314` 可作为正式运行环境使用。

已确认通过的能力:

- 主包 `editable install` 成功
- 核心模块导入通过
- `_api` 导入通过
- `pytdx` 在 Python 3.14 下可导入并实测取数成功
- `TDX` 证券列表与日线数据查询成功
- 可选分析栈已升级到 Python 3.14 可用替代实现
- `notebook / viz / web / performance` 可选组均可安装

当前未纳入“已完成”范围的只有 Rust 可选层:

- `qars3`
- `qadataswap`

它们不影响 Python 主流程迁移结果，只影响 Rust 加速能力。

## 3. 关键代码调整

### 3.1 Python 版本声明

调整文件:

- `setup.py`
- `QUANTAXIS/_api.py`

调整内容:

- 明确把支持范围更新为 Python `3.9-3.14`
- 增加 `3.13` / `3.14` classifier

### 3.2 Python 3.14 语法兼容

调整文件:

- `QUANTAXIS/QAFetch/QATdx.py`

调整内容:

- 修复无效转义字符串导致的 Python 3.14 `SyntaxWarning`

### 3.3 依赖矩阵分流

调整文件:

- `requirements.txt`

调整内容:

- 对 `numpy` / `pyarrow` / `lxml` 按 Python 版本分流
- Python `<3.14` 保留旧稳定区间
- Python `>=3.14` 改为使用具备 `cp314` wheel 的版本

核心分流策略:

- `numpy>=2.4.0,<3.0.0; python_version >= "3.14"`
- `pyarrow>=24.0.0,<25.0.0; python_version >= "3.14"`
- `lxml>=6.1.0,<7.0.0; python_version >= "3.14"`

### 3.4 可选依赖降级与替代

调整文件:

- `QUANTAXIS/QAFactor/featureAnalysis.py`
- `QUANTAXIS/QIFI/QifiManager.py`
- `setup.py`
- `requirements.txt`

调整内容:

- 将历史分析栈从基础硬依赖中拆出
- `alphalens` 改为可选依赖，缺失时不再阻塞 `_api`
- `pyfolio` 改为延迟加载，缺失时只在实际调用分析功能时报错
- Python `3.14` 使用 reloaded 替代栈:
  - `empyrical-reloaded`
  - `pyfolio-reloaded`
  - `alphalens-reloaded`

## 4. 可选依赖处理结果

### 4.1 已完成升级

#### `empyrical`

处理结果:

- Python `<3.14` 继续允许旧包
- Python `>=3.14` 改用 `empyrical-reloaded`

对项目的影响:

- 主要服务收益/风险统计分析
- 不影响抓数、存库、数据结构、TDX 下载主路径

#### `pyfolio`

处理结果:

- Python `<3.14` 继续允许旧包
- Python `>=3.14` 改用 `pyfolio-reloaded`

对项目的影响:

- 主要影响 `QIFI` 绩效分析与 tear sheet
- 不影响主包运行

#### `alphalens`

处理结果:

- Python `<3.14` 继续允许旧包
- Python `>=3.14` 改用 `alphalens-reloaded`
- 同时改成可选依赖，不再阻塞主导入

对项目的影响:

- 主要影响因子分析、forward returns、factor tear sheet
- 不影响抓数、交易、数据结构主流程

#### `IPython`

处理结果:

- 保留，放入 `notebook` extra
- Python 3.14 下已验证可安装

对项目的影响:

- 只影响交互式开发体验
- 不影响主流程

#### `jupyter`

处理结果:

- 保留，放入 `notebook` extra
- Python 3.14 下已验证可安装

对项目的影响:

- 只影响 notebook 场景
- 不影响主流程

### 4.2 保留但不再视为核心依赖

#### `pyecharts_snapshot`

处理结果:

- 仍可安装
- 保留在 `viz` / `full` extra 中
- 不再作为基础安装硬依赖

对项目的影响:

- 仅影响静态图像导出
- 不影响图表生成本身
- 不影响主流程

说明:

- 该包维护活跃度弱，后续如需稳定图像导出，建议评估替代方案

## 5. extras 策略调整

### 调整前

`full` 包含 Rust 可选层:

- `qars3`
- `qadataswap`

这会导致在当前源不可获得这两个包时，`pip install -e '.[full]'` 失败。

### 调整后

`setup.py` 中的 extra 语义调整为:

- `analysis`: 分析栈
- `notebook`: 交互式环境
- `viz`: 静态导出
- `web`: Web 扩展
- `performance`: Python 侧性能增强
- `rust`: Rust 可选层
- `full`: 可直接装通的完整 Python 可选集
- `full_rust`: 包含 Rust 层的全量扩展集

这样做的目的:

1. 保证 `.[full]` 在 Python 3.14 下可直接安装
2. 把“Rust 包不可得”与“Python 主环境不可用”彻底分离

## 6. `venv314` 安装结果

截至本记录编写时，`venv314` 已具备以下状态:

### 基础环境

- `quantaxis` editable install 成功
- CLI 入口已生成:
  - `quantaxis`
  - `quantaxisq`
  - `qarun`
  - `qawebserver`

### 已安装的可选组

- `analysis`
- `notebook`
- `viz`
- `web`
- `performance`
- `full`

### 当前未安装成功的组

- `rust`
- `full_rust`

失败原因:

- 当前源中无 `qars3>=0.0.45`
- `qadataswap` 也未作为可直接安装依赖完成验证

这属于“发布物缺失或私有依赖不可得”，不是 Python 3.14 兼容性失败。

## 7. 实际验证记录

### 7.1 smoke import

执行:

```bash
venv314/bin/python test/qaself/manual_py314_check.py --json
```

结果:

- `QUANTAXIS`
- `QUANTAXIS.QAFetch.QATdx`
- `QUANTAXIS.QAFetch.QATushare`
- `QUANTAXIS.QAFetch.QABaostock`
- `QUANTAXIS.QAData.QADataStruct`
- `QUANTAXIS.QAIndicator.indicators`
- `QUANTAXIS.QIFI.QifiManager`

全部通过。

### 7.2 `_api` 导入

执行:

```bash
venv314/bin/python test/qaself/manual_py314_check.py --include-api --json
```

结果:

- 初次失败点为 `alphalens` 缺失
- 经改造为可选依赖后，`QUANTAXIS._api` 导入通过

### 7.3 `pytdx / TDX` 实测

执行:

```bash
venv314/bin/python test/qaself/manual_py314_check.py --run-pytdx --max-servers 3 --json
```

结果:

- 成功连接 `124.71.187.122:7709`
- 成功获取证券列表
- 成功获取 `000001` 最近 5 根日线

说明:

- `pytdx` 在 Python 3.14 下不只是“可导入”，而是已通过实际取数验证

### 7.4 可选分析栈验证

执行方式:

- 安装 `analysis` extra
- 直接导入 `alphalens` / `pyfolio` / `empyrical`
- 通过 QUANTAXIS 自身入口验证 `pyfolio` 延迟加载路径

结果:

- `empyrical-reloaded` 可安装
- `pyfolio-reloaded` 可安装
- `alphalens-reloaded` 可安装
- import 名字仍兼容为:
  - `empyrical`
  - `pyfolio`
  - `alphalens`

### 7.5 关键包兼容专项检查

针对 `numpy 2.x`、`pandas 2.x`、`pyarrow`、`lxml` 做了额外巡检。

#### `numpy 2.x`

扫描结果:

- 未发现 `np.float / np.int / np.bool / np.object`
- 未发现 `np.asscalar`
- 未发现 `np.mat`
- 未发现 `np.typeDict`
- 未发现 `np.find_common_type`

结论:

- 当前仓库中未发现典型 `NumPy 2.x` 硬断点

#### `pandas 2.x`

已确认修复的兼容点:

- `QUANTAXIS/QAUtil/QABar.py`
  - 把 `date_range(..., closed='right').append(...)` 改为
    `inclusive='right'` + `union(...)`
  - 顺手修复旧日期拼接问题
- `QUANTAXIS/QAFetch/QAhuobi_realtime.py`
  - 把默认参数中的 `pd.Series()` 改为显式 dtype

扫描结果:

- 主仓代码中未继续发现 `DataFrame.append` / `Series.append` 这类仍在执行路径中的高风险点
- `iteritems` 仍存在于 `QAData/base_datastruct.py`，但这是项目自身定义的方法，不是直接调用 pandas 已移除接口

结论:

- `pandas 2.x` 主路径兼容已打通
- 当前剩余风险更多来自尚未被业务脚本覆盖到的边缘调用链，而不是全局 API 断裂

#### `pyarrow` / `lxml`

验证结果:

- `pyarrow 24.0.0` 导入通过
- `lxml 6.1.0` 导入通过

项目内主要落点:

- `QADataBridge/arrow_converter.py`
- `QADataBridge/__init__.py`
- `QARSBridge/qars_data.py`
- `QAFetch/QAThs.py`

结论:

- 当前未发现 `pyarrow` / `lxml` 在 Python 3.14 环境下的阻塞性问题

### 7.6 模块批量导入检查

对以下目录执行了批量模块导入巡检:

- `QUANTAXIS.QASU`
- `QUANTAXIS.QAFetch`
- `QUANTAXIS.QAIndicator`

结果:

- `QAIndicator`: `bad 0`
- `QAFetch`: `bad 0`
- `QASU`: 仅剩 `save_tusharepro_pg` 受控提示缺少可选 PG 依赖

额外处理:

- `QUANTAXIS/QAFetch/QAJQdata.py`
  - 改为“模块可导入，实际调用时若缺 `jqdatasdk` 再报明确错误”
- `QUANTAXIS/QASU/save_tusharepro_pg.py`
  - 去除未使用的 `talib` 顶层硬依赖
  - 对 `tushare / sqlalchemy / psycopg2` 改为可选依赖守卫
- `QUANTAXIS/QASU/test_save_strategy.py`
  - 对旧 `QAARP.QAStrategy` 路径增加兼容回退

### 7.7 编译与最小运行验证

执行过的额外验证:

- `compileall` 检查:
  - `QUANTAXIS/QASU`
  - `QUANTAXIS/QAFetch`
  - `QUANTAXIS/QAIndicator`
  - `QUANTAXIS/QAFactor`
- 最小运行验证:
  - `QABar` 的分钟/小时/期货分钟索引函数
  - `QAFetch.QAThs`
  - `QAFetch.QAfinancial`
  - `QAIndicator.base` 中典型指标函数

结果:

- 相关目录编译通过
- 最小运行样例通过

## 8. TDX 网络约束

本项目的 TDX 测试需注意网络条件:

- `TDX / pytdx` 连通性测试仅在大陆直连网络下有参考意义
- 代理、海外出口、沙箱网络下的失败，不能直接判定为迁移失败

因此:

- 网络导致的 TDX 失败应与代码兼容问题分开判断

## 9. 现阶段残留限制

### 9.1 Rust 可选层未完成

当前状态:

- `QA.__has_qars__ == False`
- `QA.__has_dataswap__ == False`

影响:

- 不影响 Python 主流程
- 不影响抓数、TDX、QATushare、QABaostock、DataStruct、`_api`
- 仅影响 Rust 账户引擎与零拷贝数据交换加速能力

### 9.2 `save_tusharepro_pg` 仍依赖可选 PostgreSQL 工具链

当前状态:

- 模块可导入
- 实际调用时若缺少以下依赖会报明确错误:
  - `sqlalchemy`
  - `psycopg2`
  - `tushare`

影响:

- 不影响主包运行
- 不影响主抓数链路
- 仅影响该 PostgreSQL 辅助脚本

### 9.3 `pyecharts_snapshot` 非强保证能力

当前状态:

- 可安装
- 不保证所有环境下静态导图都稳定

影响:

- 只影响图像导出
- 不影响主流程

## 10. 推荐使用方式

日常使用建议直接进入 `venv314`:

```bash
cd /home/bmilk/bmilk/git/quantaxis/QUANTAXIS
source venv314/bin/activate
python
```

或直接:

```bash
venv314/bin/python your_script.py
venv314/bin/quantaxis
```

不需要额外强调 `python3.14`，因为 `venv314/bin/python` 本身就是 Python `3.14.4`。

## 11. 后续建议

建议后续工作按优先级分为两类:

### 高优先级

- 用 `venv314` 跑真实业务脚本
- 验证常用 `save_*` / 下载 / 入库命令
- 若涉及图像导出，单独验证 `pyecharts_snapshot`

### 低优先级

- 获取 `qars3` / `qadataswap` 的源码或 wheel
- 单独完成 `rust` / `full_rust` 环境落地

## 12. 结论摘要

本次迁移的最终结论如下:

1. QUANTAXIS 已完成到 Python 3.14 的主流程迁移
2. `venv314` 可直接作为正式运行环境使用
3. `pytdx / TDX / opentdx` 路径已通过实际取数验证
4. 历史分析栈已通过 reloaded 替代方案适配到 Python 3.14
5. 当前唯一未完成的是 Rust 可选加速层，而不是 Python 主环境
