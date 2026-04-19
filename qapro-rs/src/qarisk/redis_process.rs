//! Redis 风险进程（Phase 5）
//!
//! 提供：
//! - 风控决策实时发布（PUBLISH）
//! - 风控事件持久化（LPUSH audit trail）
//! - 风险状态快照存取（GET/SET）
//! - Kill Switch 远程触发（SUBSCRIBE 控制频道）
//!
//! ## Redis 键约定
//!
//! | 键 / 频道                     | 用途                          |
//! |-------------------------------|-------------------------------|
//! | `risk:decisions`              | 发布频道：每笔风控决策 JSON   |
//! | `risk:alerts`                 | 发布频道：告警 / 状态升级     |
//! | `risk:control`                | 订阅频道：接收外部控制命令    |
//! | `risk:state:<account_id>`     | Hash：当前风控状态快照        |
//! | `risk:events:<account_id>`    | List：事件审计流（最新在前）  |

use std::time::{SystemTime, UNIX_EPOCH};

use redis::{Client, Commands, Connection, RedisResult};
use serde::{Deserialize, Serialize};

use crate::qarisk::service::{RiskDecision, RiskService};
use crate::qarisk::statemachine::RiskLevel;

// ─── 事件类型 ─────────────────────────────────────────────────────────────────

/// 风控系统发出的结构化事件（可序列化为 JSON）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum RiskEvent {
    /// 单笔订单风控结果
    Decision {
        timestamp_ms: i64,
        account_id: String,
        order_id: String,
        approved: bool,
        risk_level: String,
        block_reasons: Vec<String>,
        warnings: Vec<String>,
    },
    /// 风险等级变化告警
    Alert {
        timestamp_ms: i64,
        account_id: String,
        risk_level: String,
        message: String,
    },
    /// 状态机迁移事件
    StateChange {
        timestamp_ms: i64,
        account_id: String,
        from_level: String,
        to_level: String,
        reason: String,
    },
    /// Kill Switch 触发
    KillSwitch {
        timestamp_ms: i64,
        account_id: String,
        triggered_by: String,
    },
}

/// 风险状态快照（持久化到 Redis Hash）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskSnapshot {
    pub account_id: String,
    pub risk_level: String,
    pub pnl_ratio: f64,
    pub portfolio_vol: f64,
    pub kill_switch_triggered: bool,
    pub timestamp_ms: i64,
}

// ─── 控制命令（从 risk:control 频道接收） ────────────────────────────────────

/// 从外部控制频道收到的命令
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ControlCommand {
    /// 立即触发 Kill Switch
    KillSwitch { triggered_by: String },
    /// 手动提升风险等级
    Escalate { level: String, reason: String },
    /// 日初重置
    DailyReset,
}

// ─── Redis 风险进程 ───────────────────────────────────────────────────────────

/// Redis 风险进程：将 RiskService 与 Redis 打通
pub struct RiskRedisProcess {
    service: RiskService,
    account_id: String,
    conn: Connection,
    /// 事件列表最大长度（0 = 不限制）
    max_events: usize,
}

impl RiskRedisProcess {
    /// 创建并连接 Redis
    ///
    /// `redis_url` 示例：`"redis://127.0.0.1:6379/"`
    pub fn connect(
        service: RiskService,
        account_id: impl Into<String>,
        redis_url: &str,
    ) -> RedisResult<Self> {
        let client = Client::open(redis_url)?;
        let conn = client.get_connection()?;
        Ok(Self {
            service,
            account_id: account_id.into(),
            conn,
            max_events: 10_000,
        })
    }

    pub fn with_max_events(mut self, n: usize) -> Self {
        self.max_events = n;
        self
    }

    // ─── 内部工具 ─────────────────────────────────────────────────────────────

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    fn event_key(&self) -> String {
        format!("risk:events:{}", self.account_id)
    }

    fn state_key(&self) -> String {
        format!("risk:state:{}", self.account_id)
    }

    // ─── 发布接口 ─────────────────────────────────────────────────────────────

    /// 发布风控决策到 `risk:decisions` 频道，并追加到审计事件流
    pub fn publish_decision(
        &mut self,
        order_id: &str,
        decision: &RiskDecision,
    ) -> RedisResult<()> {
        let event = RiskEvent::Decision {
            timestamp_ms: Self::now_ms(),
            account_id: self.account_id.clone(),
            order_id: order_id.to_string(),
            approved: decision.approved,
            risk_level: format!("{:?}", decision.risk_level),
            block_reasons: decision.block_reasons.clone(),
            warnings: decision.warnings.clone(),
        };
        let json = serde_json::to_string(&event).unwrap_or_default();
        let _: () = self.conn.publish("risk:decisions", &json)?;
        self.append_event(&json)
    }

    /// 发布告警到 `risk:alerts` 频道
    pub fn publish_alert(&mut self, risk_level: RiskLevel, message: &str) -> RedisResult<()> {
        let event = RiskEvent::Alert {
            timestamp_ms: Self::now_ms(),
            account_id: self.account_id.clone(),
            risk_level: format!("{:?}", risk_level),
            message: message.to_string(),
        };
        let json = serde_json::to_string(&event).unwrap_or_default();
        let _: () = self.conn.publish("risk:alerts", &json)?;
        self.append_event(&json)
    }

    /// 发布状态迁移事件
    pub fn publish_state_change(
        &mut self,
        from: RiskLevel,
        to: RiskLevel,
        reason: &str,
    ) -> RedisResult<()> {
        let event = RiskEvent::StateChange {
            timestamp_ms: Self::now_ms(),
            account_id: self.account_id.clone(),
            from_level: format!("{:?}", from),
            to_level: format!("{:?}", to),
            reason: reason.to_string(),
        };
        let json = serde_json::to_string(&event).unwrap_or_default();
        let _: () = self.conn.publish("risk:alerts", &json)?;
        self.append_event(&json)
    }

    // ─── 状态快照 ─────────────────────────────────────────────────────────────

    /// 将当前风控状态保存到 Redis Hash
    pub fn save_snapshot(&mut self, pnl_ratio: f64, portfolio_vol: f64) -> RedisResult<()> {
        let snapshot = RiskSnapshot {
            account_id: self.account_id.clone(),
            risk_level: format!("{:?}", self.service.risk_level()),
            pnl_ratio,
            portfolio_vol,
            kill_switch_triggered: self.service.kill_switch().is_triggered(),
            timestamp_ms: Self::now_ms(),
        };
        let json = serde_json::to_string(&snapshot).unwrap_or_default();
        let _: () = self.conn.set(self.state_key(), json)?;
        Ok(())
    }

    /// 从 Redis 读取状态快照
    pub fn load_snapshot(&mut self) -> RedisResult<Option<RiskSnapshot>> {
        let json: Option<String> = self.conn.get(self.state_key())?;
        Ok(json.and_then(|s| serde_json::from_str(&s).ok()))
    }

    // ─── Kill Switch 远程同步 ─────────────────────────────────────────────────

    /// 将本地 Kill Switch 状态同步到 Redis（供其他进程检查）
    pub fn sync_kill_switch(&mut self) -> RedisResult<()> {
        let val = if self.service.kill_switch().is_triggered() { "1" } else { "0" };
        let key = format!("risk:kill_switch:{}", self.account_id);
        let _: () = self.conn.set(key, val)?;
        Ok(())
    }

    /// 检查 Redis 中是否有外部 Kill Switch 信号，如有则触发本地
    pub fn check_remote_kill_switch(&mut self) -> RedisResult<bool> {
        let key = format!("risk:kill_switch:{}", self.account_id);
        let val: Option<String> = self.conn.get(&key)?;
        if val.as_deref() == Some("1") {
            self.service.trigger_kill_switch();
            return Ok(true);
        }
        Ok(false)
    }

    // ─── 控制命令处理 ─────────────────────────────────────────────────────────

    /// 从 Redis List `risk:ctrl:<account_id>` 弹出并处理一条控制命令
    ///
    /// 返回 `true` 表示处理了命令，`false` 表示队列为空。
    pub fn process_one_control_cmd(&mut self, timestamp_ms: i64) -> RedisResult<bool> {
        let ctrl_key = format!("risk:ctrl:{}", self.account_id);
        let val: Option<String> = self.conn.rpop(&ctrl_key)?;
        let json = match val {
            Some(j) => j,
            None => return Ok(false),
        };
        let cmd: ControlCommand = match serde_json::from_str(&json) {
            Ok(c) => c,
            Err(_) => return Ok(false),
        };
        match cmd {
            ControlCommand::KillSwitch { triggered_by } => {
                self.service.trigger_kill_switch();
                let event = RiskEvent::KillSwitch {
                    timestamp_ms: Self::now_ms(),
                    account_id: self.account_id.clone(),
                    triggered_by,
                };
                let ev_json = serde_json::to_string(&event).unwrap_or_default();
                let _: () = self.conn.publish("risk:alerts", &ev_json)?;
                let _ = self.append_event(&ev_json);
            }
            ControlCommand::Escalate { level, reason } => {
                let rl = parse_risk_level(&level);
                self.service.escalate(rl, &reason, timestamp_ms);
            }
            ControlCommand::DailyReset => {
                self.service.daily_reset(timestamp_ms);
            }
        }
        Ok(true)
    }

    // ─── 审计流 ───────────────────────────────────────────────────────────────

    fn append_event(&mut self, json: &str) -> RedisResult<()> {
        let key = self.event_key();
        let _: () = self.conn.lpush(&key, json)?;
        if self.max_events > 0 {
            let _: () = self.conn.ltrim(&key, 0, self.max_events as isize - 1)?;
        }
        Ok(())
    }

    /// 读取最近 N 条审计事件
    pub fn recent_events(&mut self, n: isize) -> RedisResult<Vec<String>> {
        self.conn.lrange(self.event_key(), 0, n - 1)
    }

    // ─── 代理 RiskService 核心方法 ────────────────────────────────────────────

    /// 评估订单（代理 `RiskService::evaluate`）
    pub fn service(&self) -> &RiskService {
        &self.service
    }

    pub fn service_mut(&mut self) -> &mut RiskService {
        &mut self.service
    }
}

// ─── 辅助 ─────────────────────────────────────────────────────────────────────

fn parse_risk_level(s: &str) -> RiskLevel {
    match s.to_uppercase().as_str() {
        "WARNING" => RiskLevel::Warning,
        "RESTRICT" => RiskLevel::Restrict,
        "LIQUIDATE" => RiskLevel::Liquidate,
        "HALT" => RiskLevel::Halt,
        _ => RiskLevel::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qarisk::service::RiskService;
    use crate::qarisk::market::MarketType;
    use crate::qarisk::statemachine::RiskLevel;

    #[test]
    fn test_risk_event_serialize() {
        let event = RiskEvent::Decision {
            timestamp_ms: 1_700_000_000_000,
            account_id: "acc1".into(),
            order_id: "o1".into(),
            approved: true,
            risk_level: "Normal".into(),
            block_reasons: vec![],
            warnings: vec!["接近仓位上限".into()],
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event_type\":\"decision\""));
        assert!(json.contains("\"approved\":true"));
    }

    #[test]
    fn test_risk_snapshot_roundtrip() {
        let snap = RiskSnapshot {
            account_id: "acc1".into(),
            risk_level: "Normal".into(),
            pnl_ratio: -0.03,
            portfolio_vol: 0.15,
            kill_switch_triggered: false,
            timestamp_ms: 1_700_000_000_000,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let snap2: RiskSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap2.account_id, "acc1");
        assert!((snap2.pnl_ratio - (-0.03)).abs() < 1e-10);
    }

    #[test]
    fn test_control_command_serialize() {
        let cmd = ControlCommand::KillSwitch {
            triggered_by: "ops@admin".into(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"cmd\":\"KILL_SWITCH\""));
        let cmd2: ControlCommand = serde_json::from_str(&json).unwrap();
        match cmd2 {
            ControlCommand::KillSwitch { triggered_by } => {
                assert_eq!(triggered_by, "ops@admin");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_control_command_daily_reset() {
        let cmd = ControlCommand::DailyReset;
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("DAILY_RESET"));
        let cmd2: ControlCommand = serde_json::from_str(&json).unwrap();
        matches!(cmd2, ControlCommand::DailyReset);
    }

    #[test]
    fn test_parse_risk_level() {
        assert_eq!(parse_risk_level("WARNING"), RiskLevel::Warning);
        assert_eq!(parse_risk_level("HALT"), RiskLevel::Halt);
        assert_eq!(parse_risk_level("unknown"), RiskLevel::Normal);
    }

    /// 需要运行中的 Redis 实例（127.0.0.1:6379）
    #[ignore = "requires running Redis"]
    #[test]
    fn test_redis_publish_decision() {
        let svc = RiskService::new(MarketType::CN, 100_000.0);
        let mut proc = RiskRedisProcess::connect(svc, "test_acc", "redis://127.0.0.1:6379/")
            .expect("redis connection failed");

        let decision = crate::qarisk::service::RiskDecision {
            approved: true,
            risk_level: RiskLevel::Normal,
            block_reasons: vec![],
            warnings: vec![],
        };
        proc.publish_decision("order_001", &decision).unwrap();
        let events = proc.recent_events(1).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].contains("order_001"));
    }

    #[ignore = "requires running Redis"]
    #[test]
    fn test_redis_snapshot_roundtrip() {
        let svc = RiskService::new(MarketType::CN, 100_000.0);
        let mut proc = RiskRedisProcess::connect(svc, "test_snap", "redis://127.0.0.1:6379/")
            .expect("redis connection failed");

        proc.save_snapshot(-0.02, 0.18).unwrap();
        let snap = proc.load_snapshot().unwrap().expect("snapshot not found");
        assert_eq!(snap.account_id, "test_snap");
        assert!((snap.pnl_ratio - (-0.02)).abs() < 1e-10);
    }
}
