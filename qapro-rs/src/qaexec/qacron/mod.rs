//! QA Cron：基于 cron 表达式的定时任务调度
//!
//! 提供轻量级的定时任务注册与执行能力，无外部依赖。
//! 时间精度：分钟级（兼容标准 5 字段 cron）。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Cron 字段（分/时/日/月/周）
#[derive(Debug, Clone)]
pub enum CronField {
    Any,
    Value(u32),
    List(Vec<u32>),
    Range(u32, u32),
    Step(u32),   // */step
}

impl CronField {
    pub fn matches(&self, v: u32) -> bool {
        match self {
            CronField::Any => true,
            CronField::Value(n) => *n == v,
            CronField::List(ns) => ns.contains(&v),
            CronField::Range(lo, hi) => v >= *lo && v <= *hi,
            CronField::Step(s) => *s > 0 && v % s == 0,
        }
    }

    fn parse(s: &str) -> Self {
        if s == "*" { return CronField::Any; }
        if s.starts_with("*/") {
            if let Ok(n) = s[2..].parse::<u32>() { return CronField::Step(n); }
        }
        if s.contains('-') {
            let parts: Vec<&str> = s.splitn(2, '-').collect();
            if let (Ok(lo), Ok(hi)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                return CronField::Range(lo, hi);
            }
        }
        if s.contains(',') {
            let ns: Vec<u32> = s.split(',').filter_map(|p| p.parse().ok()).collect();
            if !ns.is_empty() { return CronField::List(ns); }
        }
        if let Ok(n) = s.parse::<u32>() { return CronField::Value(n); }
        CronField::Any
    }
}

/// 解析后的 cron 表达式（分 时 日 月 周）
#[derive(Debug, Clone)]
pub struct CronExpr {
    pub minute: CronField,
    pub hour: CronField,
    pub day: CronField,
    pub month: CronField,
    pub weekday: CronField,
}

impl CronExpr {
    /// 解析 5 字段 cron 字符串（"分 时 日 月 周"）
    pub fn parse(expr: &str) -> Option<Self> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 { return None; }
        Some(Self {
            minute: CronField::parse(parts[0]),
            hour: CronField::parse(parts[1]),
            day: CronField::parse(parts[2]),
            month: CronField::parse(parts[3]),
            weekday: CronField::parse(parts[4]),
        })
    }

    /// 判断给定的 (minute, hour, day, month, weekday) 是否触发
    pub fn matches(&self, minute: u32, hour: u32, day: u32, month: u32, weekday: u32) -> bool {
        self.minute.matches(minute)
            && self.hour.matches(hour)
            && self.day.matches(day)
            && self.month.matches(month)
            && self.weekday.matches(weekday)
    }
}

/// 单个定时任务
pub struct CronJob {
    pub id: String,
    pub expr: CronExpr,
    pub task: Box<dyn Fn() + Send + Sync>,
}

/// 定时任务调度器
pub struct QACron {
    jobs: Arc<Mutex<BTreeMap<String, CronJob>>>,
}

impl Default for QACron {
    fn default() -> Self {
        Self { jobs: Arc::new(Mutex::new(BTreeMap::new())) }
    }
}

impl QACron {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册任务；`expr` 为 5 字段 cron 字符串
    pub fn add<F>(&self, id: &str, expr: &str, f: F) -> Result<(), String>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let cron = CronExpr::parse(expr)
            .ok_or_else(|| format!("invalid cron expr: {}", expr))?;
        let job = CronJob { id: id.to_string(), expr: cron, task: Box::new(f) };
        self.jobs.lock().unwrap().insert(id.to_string(), job);
        Ok(())
    }

    /// 移除任务
    pub fn remove(&self, id: &str) {
        self.jobs.lock().unwrap().remove(id);
    }

    /// 检查当前系统时间，触发所有匹配的任务（同步，适用于单次轮询）
    pub fn tick(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        // 简单时间分解（UTC）
        let total_min = now / 60;
        let minute = (total_min % 60) as u32;
        let total_hour = total_min / 60;
        let hour = (total_hour % 24) as u32;
        let total_day = total_hour / 24;
        let weekday = ((total_day + 4) % 7) as u32; // 1970-01-01 是周四
        // 粗略月/日（不考虑闰年，仅用于演示）
        let day = ((total_day % 30) + 1) as u32;
        let month = ((total_day / 30 % 12) + 1) as u32;

        let jobs = self.jobs.lock().unwrap();
        for job in jobs.values() {
            if job.expr.matches(minute, hour, day, month, weekday) {
                (job.task)();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_parse_any() {
        let expr = CronExpr::parse("* * * * *").unwrap();
        assert!(expr.matches(0, 0, 1, 1, 0));
        assert!(expr.matches(59, 23, 31, 12, 6));
    }

    #[test]
    fn test_cron_parse_specific() {
        let expr = CronExpr::parse("30 9 * * 1").unwrap();
        assert!(expr.matches(30, 9, 15, 6, 1));
        assert!(!expr.matches(0, 9, 15, 6, 1));
        assert!(!expr.matches(30, 9, 15, 6, 2));
    }

    #[test]
    fn test_cron_step() {
        let expr = CronExpr::parse("*/15 * * * *").unwrap();
        assert!(expr.matches(0, 0, 1, 1, 0));
        assert!(expr.matches(15, 0, 1, 1, 0));
        assert!(expr.matches(30, 0, 1, 1, 0));
        assert!(!expr.matches(7, 0, 1, 1, 0));
    }
}
