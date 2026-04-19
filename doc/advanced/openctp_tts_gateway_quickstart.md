# OpenCTP TTS 行情网关联调说明

更新时间: 2026-04-19
适用范围: `qapro-rs/examples/openctp_md_gateway.rs`

## 1. 目标

用最少人工准备跑通这条链路:

```text
openctp TTS
  -> ctp2rs
  -> CTPMdSource
  -> PullSourcePump
  -> MarketDataDistributor
```

当前这条链路已经具备:

- `CTPMdSource` 登录/订阅/回调映射
- `PullSourcePump` 拉模式桥接
- `MarketDataDistributor` 分发入口
- 最小启动示例 `openctp_md_gateway`

## 2. 需要人工提供的内容

目前仍然必须人工提供的只有两类信息:

1. CTP 账号参数
- `OPENCTP_USER_ID`
- `OPENCTP_PASSWORD`
- 如柜台要求，再补 `OPENCTP_APP_ID`
- 如柜台要求，再补 `OPENCTP_AUTH_CODE`

2. 本机动态库路径
- `OPENCTP_MD_DYNLIB`
- `OPENCTP_TD_DYNLIB`

除这两类之外，其余参数仓库里都已经给了默认模板。

## 3. 环境变量模板

模板文件:

- [openctp_md_gateway.env.example](/home/bmilk/bmilk/git/quantaxis/QUANTAXIS/qapro-rs/examples/openctp_md_gateway.env.example)

最小需要修改:

- `OPENCTP_USER_ID`
- `OPENCTP_PASSWORD`
- `OPENCTP_MD_DYNLIB`
- `OPENCTP_TD_DYNLIB`

如果你使用的不是默认 TTS 前置，再改:

- `OPENCTP_MD_FRONT`
- `OPENCTP_TD_FRONT`

## 4. 启动方式

在仓库根目录执行:

```bash
set -a
source qapro-rs/examples/openctp_md_gateway.env.example
set +a

cargo run --manifest-path qapro-rs/Cargo.toml --example openctp_md_gateway --features openctp
```

## 5. 预期结果

启动成功后，至少应满足以下几点:

1. 进程能正常启动，不在 `MdApi::create_api` 阶段报错
2. 能完成前置连接
3. 能完成用户登录
4. 能对 `OPENCTP_INSTRUMENTS` 中的合约发起订阅
5. 能持续收到行情回调

当前示例会输出:

```text
openctp md gateway started, instruments=[...]
```

后续如果需要更清晰的联调日志，可以再补:

- 登录成功日志
- 订阅成功日志
- 首笔 tick 日志

## 6. 失败时优先排查

### 动态库问题

常见现象:

- `MdApi::create_api panic`
- 启动即退出

优先检查:

- `OPENCTP_MD_DYNLIB`
- `OPENCTP_TD_DYNLIB`
- 动态库版本是否与 openctp 环境匹配

### 账户或前置问题

常见现象:

- 可以连前置，但登录失败
- 登录成功，但订阅失败

优先检查:

- `OPENCTP_BROKER_ID`
- `OPENCTP_USER_ID`
- `OPENCTP_PASSWORD`
- `OPENCTP_MD_FRONT`
- `OPENCTP_TD_FRONT`
- `OPENCTP_APP_ID`
- `OPENCTP_AUTH_CODE`

### 合约问题

常见现象:

- 登录正常，但没有 tick

优先检查:

- `OPENCTP_INSTRUMENTS` 是否为当前可订阅合约
- 合约是否处于有行情时段

## 7. 当前代码位置

核心文件:

- [ctptrader.rs](/home/bmilk/bmilk/git/quantaxis/QUANTAXIS/qapro-rs/src/qamarket/qareal/ctptrader.rs)
- [pull_source_pump.rs](/home/bmilk/bmilk/git/quantaxis/QUANTAXIS/qapro-rs/src/qamarket/qamdgateway/actors/pull_source_pump.rs)
- [openctp_md_gateway.rs](/home/bmilk/bmilk/git/quantaxis/QUANTAXIS/qapro-rs/examples/openctp_md_gateway.rs)

## 8. 下一步

这份说明完成后，最合理的下一步不再是继续补骨架，而是做一次真实 TTS 联调，验证:

1. 登录是否成功
2. 首笔 tick 是否收到
3. `MarketDataDistributor` 是否已进入稳定收数状态

这一步需要你提供真实账号和本机动态库路径。
