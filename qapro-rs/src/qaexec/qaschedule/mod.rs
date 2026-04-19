//! QA Schedule：任务调度管理器
//!
//! 整合 [`qacron`] 和 [`qadag`]，提供统一的调度入口：
//! - 周期性触发（cron）
//! - 依赖驱动执行（DAG）
//! - 命名任务注册与手动触发

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::qacron::QACron;
use super::qadag::QADag;

/// 任务状态
#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Succeeded,
    Failed(String),
}

type TaskFn = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

/// 调度管理器
pub struct QASchedule {
    cron: QACron,
    tasks: Arc<Mutex<BTreeMap<String, TaskFn>>>,
    statuses: Arc<Mutex<BTreeMap<String, TaskStatus>>>,
}

impl Default for QASchedule {
    fn default() -> Self {
        Self {
            cron: QACron::new(),
            tasks: Arc::new(Mutex::new(BTreeMap::new())),
            statuses: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

impl QASchedule {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个命名任务
    pub fn register<F>(&self, name: &str, f: F)
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.tasks.lock().unwrap().insert(name.to_string(), Arc::new(f));
        self.statuses.lock().unwrap().insert(name.to_string(), TaskStatus::Pending);
    }

    /// 用 cron 表达式绑定已注册的命名任务（每次 tick 时自动触发）
    pub fn schedule(&self, name: &str, cron_expr: &str) -> Result<(), String> {
        let tasks = Arc::clone(&self.tasks);
        let statuses = Arc::clone(&self.statuses);
        let name_owned = name.to_string();
        self.cron.add(name, cron_expr, move || {
            // 取出 Arc，释放锁后再调用
            let f: Option<TaskFn> = tasks.lock().unwrap().get(&name_owned).cloned();
            if let Some(f) = f {
                statuses.lock().unwrap().insert(name_owned.clone(), TaskStatus::Running);
                match f() {
                    Ok(()) => {
                        statuses.lock().unwrap().insert(name_owned.clone(), TaskStatus::Succeeded);
                    }
                    Err(e) => {
                        statuses.lock().unwrap().insert(name_owned.clone(), TaskStatus::Failed(e));
                    }
                }
            }
        })
    }

    /// 手动立即触发一个命名任务
    pub fn trigger(&self, name: &str) -> Result<(), String> {
        // 克隆 Arc，释放锁后调用
        let f: Option<TaskFn> = self.tasks.lock().unwrap().get(name).cloned();
        match f {
            None => Err(format!("task '{}' not found", name)),
            Some(f) => {
                self.statuses.lock().unwrap().insert(name.to_string(), TaskStatus::Running);
                match f() {
                    Ok(()) => {
                        self.statuses.lock().unwrap().insert(name.to_string(), TaskStatus::Succeeded);
                        Ok(())
                    }
                    Err(e) => {
                        self.statuses.lock().unwrap().insert(
                            name.to_string(),
                            TaskStatus::Failed(e.clone()),
                        );
                        Err(e)
                    }
                }
            }
        }
    }

    /// 获取任务状态
    pub fn status(&self, name: &str) -> Option<TaskStatus> {
        self.statuses.lock().unwrap().get(name).cloned()
    }

    /// 运行一次 cron 轮询
    pub fn tick(&self) {
        self.cron.tick();
    }

    /// 将已注册的任务组织为 DAG 并按拓扑顺序执行
    ///
    /// `nodes`：`(task_name, &[dependency_names])` 列表
    pub fn run_dag(&self, nodes: &[(&str, &[&str])]) -> Result<(), String> {
        let mut dag = QADag::new();
        for (name, _) in nodes {
            // 验证任务已注册
            if !self.tasks.lock().unwrap().contains_key(*name) {
                return Err(format!("task '{}' not registered", name));
            }
            let name_s = name.to_string();
            dag.add_node(name, move || {
                log::debug!("dag schedule node: {}", name_s);
                Ok(())
            });
        }
        for (name, deps) in nodes {
            for dep in *deps {
                dag.add_edge(name, dep)?;
            }
        }
        let order = dag.topological_sort()?;
        for node_id in &order {
            self.trigger(node_id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_trigger() {
        let sched = QASchedule::new();
        sched.register("hello", || Ok(()));
        assert!(sched.trigger("hello").is_ok());
        assert_eq!(sched.status("hello"), Some(TaskStatus::Succeeded));
    }

    #[test]
    fn test_trigger_not_found() {
        let sched = QASchedule::new();
        assert!(sched.trigger("nonexistent").is_err());
    }

    #[test]
    fn test_dag_execution() {
        use std::sync::{Arc, Mutex};
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let sched = QASchedule::new();

        let l1 = log.clone();
        sched.register("fetch", move || { l1.lock().unwrap().push("fetch".into()); Ok(()) });
        let l2 = log.clone();
        sched.register("process", move || { l2.lock().unwrap().push("process".into()); Ok(()) });

        sched.run_dag(&[("fetch", &[]), ("process", &["fetch"])]).unwrap();

        let result = log.lock().unwrap();
        assert_eq!(&*result, &["fetch", "process"]);
    }

    #[test]
    fn test_failed_task_status() {
        let sched = QASchedule::new();
        sched.register("fail", || Err("oops".to_string()));
        assert!(sched.trigger("fail").is_err());
        assert!(matches!(sched.status("fail"), Some(TaskStatus::Failed(_))));
    }
}
