# openctp + ctp2rs 接入设计

更新时间: 2026-04-19
范围: 第一阶段 `CTP(openctp)` 主链路

## 1. 主路线

第一阶段 CTP 主链路固定为:

```text
openctp TTS
  -> ctp2rs(upstream)
  -> CTPMdSource / CTPTrader
  -> LiveEngine
```

依赖约束:

- 优先使用上游 `pseudocodes/ctp2rs`
- 不使用已落后的 fork 作为主依赖

## 2. 验收环境

第一阶段默认验收环境:

- `openctp TTS`

原因:

- 比 SimNow 更稳定
- 更适合持续联调
- 更适合自动化测试脚本

## 3. 第一阶段交付边界

### 必做

- `CTPMdSource`
- `CTPTrader`
- 动态库路径配置
- TTS 登录验证
- 今昨仓和平仓逻辑
- 日初三查

### 非第一阶段主交付

- 自动降级到 ctpbee
- 多 CTP 发行版兼容
- 高频优化

## 4. 风险点

### 动态库部署

需要明确:

- `.so/.dll` 路径
- 本地开发机配置
- 新机器复现方式

### 今昨仓拆分

上期所场景必须单独验证:

- 昨仓平仓
- 今仓平仓
- 混合持仓拆分

### 日初状态重建

必须执行:

- `QueryAccount`
- `QueryPosition`
- `QueryOrder`

## 5. 备用预案

Python `ctpbee bridge` 仅作备用预案:

- 用于应急联调
- 用于主链路故障时的人工切换预案

不作为第一阶段主实现。
