//! 模型调用日志：每次 LLM 调用完整输入输出落 JSONL（按项目一文件）
//!
//! 路径：`{data_dir}/llm-logs/{repo_id}.jsonl`；单文件超 50MB 轮转为
//! `.old`（保留一代）。日志是本机调试数据，不进 `.bytewiki/`（产物目录
//! 会被分享/克隆）。读取只取尾部 N 条（最新在后），坏行跳过。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::service::AppService;
use crate::{CoreError, CoreResult};

/// 单文件轮转阈值
const ROTATE_BYTES: u64 = 50 * 1024 * 1024;
/// 默认读取条数（尾部）
pub const DEFAULT_TAIL: usize = 100;

/// 一次 LLM 调用的完整记录（JSONL 一行）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmLogEntry {
    /// ISO 时间
    pub ts: String,
    pub repo_id: i64,
    pub repo_name: String,
    /// outline / page / page-retry / overview / chat
    pub stage: String,
    /// 供应商（id · 名称）
    pub provider: String,
    pub model: String,
    /// 页面标题 / 问题摘要
    pub title: String,
    /// 第几次尝试（1 起；>1 表示重试）
    pub attempt: u32,
    /// 耗时（毫秒）
    pub duration_ms: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub ok: bool,
    /// 响应来自缓存回放（未真实请求）
    #[serde(default)]
    pub cached: bool,
    #[serde(default)]
    pub error: String,
    /// 完整 system prompt
    pub system: String,
    /// 完整 user prompt（多轮取拼接）
    pub user: String,
    /// 完整响应文本
    pub response: String,
}

fn logs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("llm-logs")
}

fn log_path(data_dir: &Path, repo_id: i64) -> PathBuf {
    logs_dir(data_dir).join(format!("{repo_id}.jsonl"))
}

/// 追加一条；文件超阈值时先轮转（.jsonl → .jsonl.old，旧的 .old 覆盖）
pub fn append(data_dir: &Path, entry: &LlmLogEntry) -> CoreResult<()> {
    let dir = logs_dir(data_dir);
    std::fs::create_dir_all(&dir)?;
    let path = log_path(data_dir, entry.repo_id);
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > ROTATE_BYTES {
            let old = path.with_extension("jsonl.old");
            let _ = std::fs::remove_file(&old);
            let _ = std::fs::rename(&path, &old);
        }
    }
    let mut line = serde_json::to_string(entry)
        .map_err(|e| CoreError::Other(anyhow::anyhow!("日志序列化失败：{e}")))?;
    line.push('\n');
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    f.write_all(line.as_bytes())
        .map_err(|e| CoreError::Other(anyhow::anyhow!("日志写入失败：{e}")))?;
    Ok(())
}

/// 读尾部条目（返回最新在后；坏行跳过）
pub fn read_tail(data_dir: &Path, repo_id: i64, limit: usize) -> CoreResult<Vec<LlmLogEntry>> {
    let path = log_path(data_dir, repo_id);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(vec![]);
    };
    let mut all: Vec<LlmLogEntry> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<LlmLogEntry>(line) {
            Ok(e) => all.push(e),
            Err(_) => continue, // 版本演进/损坏行：跳过不致命
        }
    }
    if all.len() > limit {
        all = all.split_off(all.len() - limit);
    }
    Ok(all)
}

/// 清空（删除当前与轮转文件）
pub fn clear(data_dir: &Path, repo_id: i64) -> CoreResult<()> {
    let path = log_path(data_dir, repo_id);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let old = path.with_extension("jsonl.old");
    if old.exists() {
        let _ = std::fs::remove_file(&old);
    }
    Ok(())
}

/// 日志文件体积（字节；不存在为 0）
pub fn log_size(data_dir: &Path, repo_id: i64) -> u64 {
    std::fs::metadata(log_path(data_dir, repo_id)).map(|m| m.len()).unwrap_or(0)
}

impl AppService {
    /// 尾部日志（最新在后）+ 当前文件体积
    pub fn list_llm_logs(&self, repo_path: &str, limit: usize) -> CoreResult<(Vec<LlmLogEntry>, u64)> {
        let canon = crate::canonical_key(std::path::Path::new(repo_path))?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Ok((vec![], 0));
        };
        Ok((read_tail(self.data_dir(), repo.id, limit)?, log_size(self.data_dir(), repo.id)))
    }

    /// 清空项目日志
    pub fn clear_llm_logs(&self, repo_path: &str) -> CoreResult<()> {
        let canon = crate::canonical_key(std::path::Path::new(repo_path))?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Ok(());
        };
        clear(self.data_dir(), repo.id)
    }

    /// 追加（仅 crate 内管线使用；失败只告警不影响主流程）
    pub(crate) fn append_llm_log(&self, entry: &LlmLogEntry) {
        if let Err(e) = append(self.data_dir(), entry) {
            tracing::warn!("模型日志写入失败（不影响主流程）：{e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: &str, title: &str) -> LlmLogEntry {
        LlmLogEntry {
            ts: format!("2026-09-15T10:00:{n}"),
            repo_id: 7,
            repo_name: "demo".into(),
            stage: "page".into(),
            provider: "p1 · 测试".into(),
            model: "mock".into(),
            title: title.into(),
            attempt: 1,
            duration_ms: 1234,
            prompt_tokens: 100,
            completion_tokens: 20,
            ok: true,
            cached: false,
            error: String::new(),
            system: "SYS".into(),
            user: format!("USER-{n}"),
            response: format!("RESP-{n}"),
        }
    }

    #[test]
    fn append_read_roundtrip_and_tail_limit() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["00", "01", "02", "03"] {
            append(dir.path(), &entry(n, "页")).unwrap();
        }
        let all = read_tail(dir.path(), 7, 10).unwrap();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].user, "USER-00"); // 顺序保留（旧→新）
        assert_eq!(all[3].response, "RESP-03");
        let tail = read_tail(dir.path(), 7, 2).unwrap();
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].user, "USER-02");
    }

    #[test]
    fn bad_lines_skipped_and_clear_works() {
        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), &entry("00", "页")).unwrap();
        let path = log_path(dir.path(), 7);
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "not-json").unwrap();
        append(dir.path(), &entry("01", "页")).unwrap();
        let all = read_tail(dir.path(), 7, 10).unwrap();
        assert_eq!(all.len(), 2);
        assert!(log_size(dir.path(), 7) > 0);
        clear(dir.path(), 7).unwrap();
        assert!(read_tail(dir.path(), 7, 10).unwrap().is_empty());
        assert_eq!(log_size(dir.path(), 7), 0);
    }
}
