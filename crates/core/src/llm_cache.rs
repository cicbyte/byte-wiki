//! LLM 响应缓存（deepwiki-rs CacheManager 思路）：同一 prompt 不重复请求
//!
//! 键 = MD5(provider + model + system + user)，按项目分目录：
//! `{data_dir}/llm-cache/{repo_id}/{hash}.json`（一条一文件，含响应、token
//! 用量与时间戳）。命中直接回放文本（token 记 0，未真实消耗）；损坏文件
//! 视为未命中。开发阶段反复重跑生成管线时，未变更的页面/大纲不再花钱。
//!
//! 语义安全性：prompt 内嵌仓库文件内容与大纲 brief——文件或 prompt 变了
//! 键就变，天然增量；模型/供应商变了键也变，不串味。

use std::path::{Path, PathBuf};

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::service::AppService;
use crate::CoreResult;

/// 缓存条目（一条一 JSON 文件）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCacheEntry {
    pub prompt_hash: String,
    /// 完整响应文本
    pub response: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub model: String,
    /// UNIX 秒
    pub timestamp: u64,
}

/// 缓存键：供应商 + 模型 + 完整消息参与哈希。
/// 每段带字节长度前缀，杜绝「内容含分隔符」造成的拼接歧义。
pub fn cache_key(provider: &str, model: &str, system: &str, user: &str) -> String {
    let mut hasher = Md5::new();
    for part in [provider, model, system, user] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn cache_dir(data_dir: &Path, repo_id: i64) -> PathBuf {
    data_dir.join("llm-cache").join(repo_id.to_string())
}

/// 查询缓存；文件不存在/损坏返回 None
pub fn get(data_dir: &Path, repo_id: i64, key: &str) -> Option<LlmCacheEntry> {
    let path = cache_dir(data_dir, repo_id).join(format!("{key}.json"));
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<LlmCacheEntry>(&text) {
        Ok(e) if e.prompt_hash == key => Some(e),
        _ => None, // 损坏/哈希不符：视为未命中
    }
}

/// 写入（best-effort：失败只告警语义由调用方决定）
pub fn set(data_dir: &Path, repo_id: i64, entry: &LlmCacheEntry) -> CoreResult<()> {
    let dir = cache_dir(data_dir, repo_id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", entry.prompt_hash));
    std::fs::write(path, serde_json::to_string_pretty(entry).map_err(|e| {
        crate::CoreError::Other(anyhow::anyhow!("缓存序列化失败：{e}"))
    })?)?;
    Ok(())
}

/// 统计（条数， 总字节）
pub fn stats(data_dir: &Path, repo_id: i64) -> (usize, u64) {
    let dir = cache_dir(data_dir, repo_id);
    let mut count = 0usize;
    let mut bytes = 0u64;
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for entry in rd.flatten() {
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                count += 1;
                bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    (count, bytes)
}

/// 清空项目缓存
pub fn clear(data_dir: &Path, repo_id: i64) -> CoreResult<()> {
    let dir = cache_dir(data_dir, repo_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

impl AppService {
    /// 缓存统计（条数， 总字节）
    pub fn llm_cache_stats(&self, repo_path: &str) -> CoreResult<(usize, u64)> {
        let Some(repo_id) = self.repo_id_of(repo_path) else {
            return Ok((0, 0));
        };
        Ok(stats(self.data_dir(), repo_id))
    }

    /// 清空项目缓存
    pub fn clear_llm_cache(&self, repo_path: &str) -> CoreResult<()> {
        let Some(repo_id) = self.repo_id_of(repo_path) else {
            return Ok(());
        };
        clear(self.data_dir(), repo_id)
    }

    fn repo_id_of(&self, repo_path: &str) -> Option<i64> {
        let canon = crate::canonical_key(Path::new(repo_path)).ok()?;
        self.db.get_repo_by_path(&canon).ok()?.map(|r| r.id)
    }

    pub(crate) fn cache_get(&self, repo_id: i64, key: &str) -> Option<LlmCacheEntry> {
        if !self
            .llm_cache_enabled
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return None;
        }
        get(self.data_dir(), repo_id, key)
    }

    pub(crate) fn cache_set(&self, repo_id: i64, entry: &LlmCacheEntry) {
        if !self
            .llm_cache_enabled
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        if let Err(e) = set(self.data_dir(), repo_id, entry) {
            tracing::warn!("缓存写入失败（不影响主流程）：{e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_roundtrip_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let key = cache_key("p1", "m1", "SYS", "USER");
        assert!(get(dir.path(), 7, &key).is_none());
        set(
            dir.path(),
            7,
            &LlmCacheEntry {
                prompt_hash: key.clone(),
                response: "RESP".into(),
                prompt_tokens: 10,
                completion_tokens: 5,
                model: "m1".into(),
                timestamp: 0,
            },
        )
        .unwrap();
        let hit = get(dir.path(), 7, &key).unwrap();
        assert_eq!(hit.response, "RESP");
        let (count, bytes) = stats(dir.path(), 7);
        assert_eq!(count, 1);
        assert!(bytes > 0);
        clear(dir.path(), 7).unwrap();
        assert_eq!(stats(dir.path(), 7).0, 0);
        assert!(get(dir.path(), 7, &key).is_none());
    }

    #[test]
    fn key_covers_provider_model_and_prompt() {
        let base = cache_key("p1", "m1", "SYS", "USER");
        assert_ne!(base, cache_key("p2", "m1", "SYS", "USER"), "供应商不同应失效");
        assert_ne!(base, cache_key("p1", "m2", "SYS", "USER"), "模型不同应失效");
        assert_ne!(base, cache_key("p1", "m1", "SYS2", "USER"), "system 不同应失效");
        assert_ne!(base, cache_key("p1", "m1", "SYS", "USER2"), "user 不同应失效");
        // 拼接歧义防护：长度前缀保证不同切分不撞键
        assert_ne!(cache_key("p", "m", "ab", "c"), cache_key("p", "m", "a", "bc"));
        assert_ne!(cache_key("p", "m", "a", "bc"), cache_key("p", "mn", "a", "c"));
    }

    #[test]
    fn corrupt_entry_is_miss() {
        let dir = tempfile::tempdir().unwrap();
        let key = cache_key("p", "m", "S", "U");
        let d = cache_dir(dir.path(), 7);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(format!("{key}.json")), "not json").unwrap();
        assert!(get(dir.path(), 7, &key).is_none());
        // 哈希不符同样视为未命中
        std::fs::write(
            d.join(format!("{key}.json")),
            serde_json::to_string(&LlmCacheEntry {
                prompt_hash: "other".into(),
                response: String::new(),
                prompt_tokens: 0,
                completion_tokens: 0,
                model: "m".into(),
                timestamp: 0,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(get(dir.path(), 7, &key).is_none());
    }
}
