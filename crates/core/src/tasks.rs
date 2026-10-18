//! 生成任务注册表：页面销毁后可重入的后台任务状态
//!
//! 生成命令在 Rust 侧独立于前端页面运行（切页/离开项目不中断），但进度
//! 状态原先只活在 RepoSpace 组件里——离开即不可见、跨项目事件互相污染。
//! 注册表持有每个项目一个任务的实时快照（阶段/进度/页面骨架状态），
//! 事件到达即更新；任意页面挂载时拉快照水合，后台任务页全局可见。

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;

use crate::generator::{GenerateEvent, PageProgress};

/// 单个生成任务的实时快照
#[derive(Debug, Clone, Serialize)]
pub struct GenTask {
    pub repo_id: i64,
    /// 任务归属分支（生成启动时捕获——归属唯一确定，不依赖事后推断）
    pub branch: String,
    pub repo_name: String,
    pub repo_path: String,
    /// generate / update
    pub kind: String,
    /// 当前阶段（indexing / outlining / generating / assembling / embedding / done / error）
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub message: String,
    /// 页面骨架状态（按 id 合并，同前端语义）
    pub pages: Vec<PageProgress>,
    /// 开始时间（UNIX 毫秒）
    pub started_ms: u64,
    /// 是否已结束（done / error）
    pub finished: bool,
    #[serde(default)]
    pub error: String,
    /// 累计 token（事件透传）
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

/// 全局注册表（每 (项目, 分支) 一个任务：同项目多分支任务并存不覆盖）
#[derive(Default)]
pub struct TaskManager {
    inner: Mutex<HashMap<(i64, String), GenTask>>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl TaskManager {
    pub fn begin(&self, repo_id: i64, branch: &str, repo_name: &str, repo_path: &str, kind: &str) {
        let mut g = self.inner.lock().unwrap();
        let key = (repo_id, branch.to_string());
        g.insert(
            key.clone(),
            GenTask {
                repo_id,
                branch: branch.to_string(),
                repo_name: repo_name.to_string(),
                repo_path: repo_path.to_string(),
                kind: kind.to_string(),
                phase: "starting".into(),
                current: 0,
                total: 0,
                message: String::new(),
                pages: vec![],
                started_ms: now_ms(),
                finished: false,
                error: String::new(),
                prompt_tokens: 0,
                completion_tokens: 0,
            },
        );
    }

    /// 事件到达即更新快照（pages 按 id 合并；outlining 全量骨架重置）。
    /// 事件带 branch（生成管线注入）时按 (repo, branch) 精确路由；
    /// 无 branch 的旧事件回退 (repo, 唯一任务) 兼容
    pub fn update(&self, repo_id: i64, e: &GenerateEvent) {
        let mut g = self.inner.lock().unwrap();
        let key = match &e.branch {
            Some(b) => (repo_id, b.clone()),
            None => {
                // 兼容：该项目的任务（多于一个时取未完成的）
                let hit = g
                    .iter()
                    .filter(|((rid, _), _)| *rid == repo_id)
                    .find(|(_, t)| !t.finished)
                    .or_else(|| g.iter().find(|((rid, _), _)| *rid == repo_id));
                match hit {
                    Some(((rid, br), _)) => (*rid, br.clone()),
                    None => return,
                }
            }
        };
        let Some(t) = g.get_mut(&key) else { return };
        t.phase = e.phase.clone();
        t.current = e.current;
        t.total = e.total;
        if !e.message.is_empty() {
            t.message = e.message.clone();
        }
        t.prompt_tokens = e.prompt_tokens;
        t.completion_tokens = e.completion_tokens;
        if let Some(pages) = &e.pages {
            if e.phase == "outlining" {
                t.pages = pages.clone();
            } else {
                for p in pages {
                    match t.pages.iter_mut().find(|x| x.id == p.id) {
                        Some(slot) => *slot = p.clone(),
                        None => t.pages.push(p.clone()),
                    }
                }
            }
        }
        if e.phase == "done" {
            t.finished = true;
        }
    }

    pub fn finish(&self, repo_id: i64, branch: &str, note: &str) {
        let mut g = self.inner.lock().unwrap();
        if let Some(t) = g.get_mut(&(repo_id, branch.to_string())) {
            t.finished = true;
            t.phase = "done".into();
            if !note.is_empty() {
                t.message = note.to_string();
            }
        }
    }

    pub fn fail(&self, repo_id: i64, branch: &str, error: &str) {
        let mut g = self.inner.lock().unwrap();
        if let Some(t) = g.get_mut(&(repo_id, branch.to_string())) {
            t.finished = true;
            t.phase = "error".into();
            t.error = error.to_string();
            t.message = format!("失败：{error}");
        }
    }

    /// 全部任务快照（运行中在前，其后按开始时间倒序）
    pub fn list(&self) -> Vec<GenTask> {
        let g = self.inner.lock().unwrap();
        let mut v: Vec<GenTask> = g.values().cloned().collect();
        v.sort_by(|a, b| a.finished.cmp(&b.finished).then(b.started_ms.cmp(&a.started_ms)));
        v
    }

    /// 清除已结束任务
    pub fn clear_finished(&self) {
        let mut g = self.inner.lock().unwrap();
        g.retain(|_, t| !t.finished);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(phase: &str, pages: Option<Vec<PageProgress>>) -> GenerateEvent {
        GenerateEvent {
            phase: phase.into(),
            current: 1,
            total: 3,
            message: "m".into(),
            prompt_tokens: 5,
            completion_tokens: 2,
            pages,
            stream: None,
            repo_id: None,
            branch: None,
        }
    }

    fn pp(id: &str, status: &str) -> PageProgress {
        PageProgress {
            id: id.into(),
            title: format!("页{id}"),
            section: "S".into(),
            importance: "high".into(),
            status: status.into(),
            path: Some(format!("pages/S/{id}.md")),
        }
    }

    #[test]
    fn lifecycle_merge_and_list() {
        let tm = TaskManager::default();
        tm.begin(1, "main", "a", "/a", "generate");
        tm.update(1, &ev("outlining", Some(vec![pp("p1", "pending"), pp("p2", "pending")])));
        tm.update(1, &ev("generating", Some(vec![pp("p1", "generating")])));
        tm.update(1, &ev("generating", Some(vec![pp("p1", "done")])));
        let list = tm.list();
        assert_eq!(list.len(), 1);
        let t = &list[0];
        assert_eq!(t.pages.len(), 2);
        assert_eq!(t.pages[0].status, "done");
        assert_eq!(t.pages[1].status, "pending");
        assert!(!t.finished);
        tm.finish(1, "main", "完成");
        assert!(tm.list()[0].finished);
        tm.clear_finished();
        assert!(tm.list().is_empty());
    }

    #[test]
    fn fail_marks_error() {
        let tm = TaskManager::default();
        tm.begin(2, "dev", "b", "/b", "update");
        tm.fail(2, "dev", "boom");
        let t = &tm.list()[0];
        assert!(t.finished);
        assert_eq!(t.phase, "error");
        assert!(t.error.contains("boom"));
    }
}
