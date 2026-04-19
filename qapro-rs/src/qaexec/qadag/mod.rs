//! QA DAG：有向无环图任务调度器
//!
//! 支持：
//! - 声明任务节点及依赖关系
//! - 拓扑排序（Kahn 算法）
//! - 顺序执行（单线程）

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// DAG 节点（任务）
pub struct DagNode {
    pub id: String,
    pub task: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
}

/// 有向无环图调度器
pub struct QADag {
    nodes: BTreeMap<String, DagNode>,
    /// 依赖关系：deps[A] = {B, C} 表示 A 依赖 B 和 C（B、C 先于 A 执行）
    deps: BTreeMap<String, BTreeSet<String>>,
}

impl Default for QADag {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            deps: BTreeMap::new(),
        }
    }
}

impl QADag {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册任务节点
    pub fn add_node<F>(&mut self, id: &str, f: F)
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.nodes.insert(
            id.to_string(),
            DagNode { id: id.to_string(), task: Box::new(f) },
        );
        self.deps.entry(id.to_string()).or_default();
    }

    /// 声明 `from` 依赖 `dep`（先执行 `dep`，再执行 `from`）
    pub fn add_edge(&mut self, from: &str, dep: &str) -> Result<(), String> {
        if from == dep {
            return Err(format!("self-loop not allowed: {}", from));
        }
        self.deps.entry(from.to_string()).or_default().insert(dep.to_string());
        self.deps.entry(dep.to_string()).or_default();
        if self.has_cycle() {
            self.deps.get_mut(from).unwrap().remove(dep);
            return Err(format!("adding edge {}->{} creates a cycle", from, dep));
        }
        Ok(())
    }

    fn has_cycle(&self) -> bool {
        let mut visited = BTreeSet::new();
        let mut rec_stack = BTreeSet::new();
        for node in self.deps.keys() {
            if self.dfs_cycle(node, &mut visited, &mut rec_stack) {
                return true;
            }
        }
        false
    }

    fn dfs_cycle(&self, node: &str, visited: &mut BTreeSet<String>, rec: &mut BTreeSet<String>) -> bool {
        if rec.contains(node) { return true; }
        if visited.contains(node) { return false; }
        visited.insert(node.to_string());
        rec.insert(node.to_string());
        if let Some(deps) = self.deps.get(node) {
            for dep in deps {
                if self.dfs_cycle(dep, visited, rec) { return true; }
            }
        }
        rec.remove(node);
        false
    }

    /// 拓扑排序（Kahn 算法）
    ///
    /// 入度 = 该节点有多少个直接依赖（即有多少节点必须先于它执行）
    pub fn topological_sort(&self) -> Result<Vec<String>, String> {
        // 收集所有节点
        let mut all: BTreeSet<String> = self.deps.keys().cloned().collect();
        for deps in self.deps.values() {
            all.extend(deps.iter().cloned());
        }

        // 入度：node 依赖多少个节点
        let mut indegree: BTreeMap<String, usize> =
            all.iter().map(|n| (n.clone(), 0)).collect();
        for (node, deps) in &self.deps {
            *indegree.entry(node.clone()).or_insert(0) += deps.len();
        }

        // Kahn：从入度为 0 的节点开始
        let mut queue: VecDeque<String> = indegree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(n, _)| n.clone())
            .collect();

        let mut order = Vec::new();
        while let Some(node) = queue.pop_front() {
            order.push(node.clone());
            // 找所有依赖 node 的后继节点，减少其入度
            for (n, deps) in &self.deps {
                if deps.contains(&node) {
                    let cnt = indegree.get_mut(n).unwrap();
                    *cnt -= 1;
                    if *cnt == 0 {
                        queue.push_back(n.clone());
                    }
                }
            }
        }

        if order.len() != all.len() {
            return Err("DAG has a cycle".to_string());
        }
        Ok(order)
    }

    /// 按拓扑顺序执行所有任务
    pub fn run(&self) -> Result<(), String> {
        let order = self.topological_sort()?;
        for node_id in &order {
            if let Some(node) = self.nodes.get(node_id) {
                (node.task)().map_err(|e| format!("task '{}' failed: {}", node_id, e))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_topological_order() {
        let mut dag = QADag::new();
        dag.add_node("a", || Ok(()));
        dag.add_node("b", || Ok(()));
        dag.add_node("c", || Ok(()));
        dag.add_edge("b", "a").unwrap(); // b 依赖 a
        dag.add_edge("c", "b").unwrap(); // c 依赖 b
        let order = dag.topological_sort().unwrap();
        let ai = order.iter().position(|x| x == "a").unwrap();
        let bi = order.iter().position(|x| x == "b").unwrap();
        let ci = order.iter().position(|x| x == "c").unwrap();
        assert!(ai < bi && bi < ci);
    }

    #[test]
    fn test_cycle_detection() {
        let mut dag = QADag::new();
        dag.add_node("a", || Ok(()));
        dag.add_node("b", || Ok(()));
        dag.add_edge("a", "b").unwrap();
        assert!(dag.add_edge("b", "a").is_err());
    }

    #[test]
    fn test_run_order() {
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut dag = QADag::new();
        let l1 = log.clone();
        dag.add_node("a", move || { l1.lock().unwrap().push("a".into()); Ok(()) });
        let l2 = log.clone();
        dag.add_node("b", move || { l2.lock().unwrap().push("b".into()); Ok(()) });
        dag.add_edge("b", "a").unwrap();
        dag.run().unwrap();
        let result = log.lock().unwrap();
        assert_eq!(result[0], "a");
        assert_eq!(result[1], "b");
    }
}
