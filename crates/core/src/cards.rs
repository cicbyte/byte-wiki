//! 知识卡（FR-07 / F16）：`cards/` 人工知识的创建与登记。
//!
//! 人工卡片写入 `.bytewiki/cards/<slug>.md`（YAML front-matter 带 title/created），
//! 登记 wiki_docs（source=human）——重新生成与增量更新都不触碰（PRD 7.3 定案）。
//! 卡片内容随向量重建进入检索（chunk_wiki_dir 已覆盖 cards/）。

use std::path::Path;

use crate::db::now_iso;
use crate::service::AppService;
use crate::{CoreError, CoreResult};

impl AppService {
    /// 创建知识卡：写文件 + 登记（human）。返回卡片相对路径。
    pub fn create_card(&self, repo_path: &str, title: &str, content: &str) -> CoreResult<String> {
        let title = title.trim();
        if title.is_empty() {
            return Err(CoreError::Other(anyhow::anyhow!("卡片标题不能为空")));
        }
        let root = Path::new(repo_path);
        let canon = crate::canonical_key(root)?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Err(CoreError::Other(anyhow::anyhow!("仓库未登记：{repo_path}")));
        };

        // slug：标题安全化 + 短时间戳防重名
        let slug = format!(
            "{}-{}",
            slugify(title),
            chrono::Utc::now().format("%Y%m%d%H%M%S")
        );
        let rel = format!("cards/{slug}.md");
        let cards_dir = root.join(".bytewiki").join("cards");
        std::fs::create_dir_all(&cards_dir)?;
        let body = format!(
            "---\ntitle: {title}\ncreated: {}\nsource: human\n---\n\n# {title}\n\n{content}\n",
            now_iso()
        );
        std::fs::write(cards_dir.join(format!("{slug}.md")), body)?;

        // 登记为 human（存在则更新时间）
        self.db.upsert_human_doc(repo.id, "card", &rel, title)?;
        self.db
            .log_change(repo.id, crate::db::SHARED_BRANCH, "card", &format!("create cards/{slug}.md"))?;
        Ok(rel)
    }

    /// 解析文档落点：cards/ 跨分支共享，其余按分支目录（canonicalize 保证不逃逸）
    fn resolve_wiki_doc(
        &self,
        repo_path: &str,
        doc_path: &str,
        branch: Option<&str>,
    ) -> CoreResult<(std::path::PathBuf, std::path::PathBuf)> {
        if doc_path.contains("..") || doc_path.starts_with('/') || doc_path.starts_with('\\') {
            return Err(CoreError::Other(anyhow::anyhow!("非法路径：{doc_path}")));
        }
        let root = Path::new(repo_path)
            .canonicalize()
            .map_err(|e| CoreError::Other(anyhow::anyhow!("仓库路径无效：{repo_path}（{e}）")))?;
        let branch = branch
            .map(str::to_string)
            .unwrap_or_else(|| crate::branch::current_branch(&root));
        let (wiki, rel) = if let Some(card) = doc_path.strip_prefix("cards/") {
            (crate::branch::cards_root(&root), card.to_string())
        } else {
            (crate::branch::wiki_root(&root, &branch), doc_path.to_string())
        };
        let full = wiki
            .join(&rel)
            .canonicalize()
            .map_err(|_| CoreError::Other(anyhow::anyhow!("文档不存在：{doc_path}")))?;
        if !full.starts_with(&wiki) {
            return Err(CoreError::Other(anyhow::anyhow!("非法路径：{doc_path}")));
        }
        Ok((wiki.join(rel), full))
    }

    /// 读 Wiki 文档原文（MCP / CLI 共用；桌面端 tauri 命令同口径）。
    /// branch 缺省 = 仓库当前分支；cards/ 前缀读共享知识卡。上限 2MB。
    pub fn read_wiki_doc(&self, repo_path: &str, doc_path: &str, branch: Option<&str>) -> CoreResult<String> {
        let (target, _) = self.resolve_wiki_doc(repo_path, doc_path, branch)?;
        let meta = std::fs::metadata(&target).map_err(|e| CoreError::Other(anyhow::anyhow!("{e}")))?;
        if meta.len() > 2 * 1024 * 1024 {
            return Err(CoreError::Other(anyhow::anyhow!(
                "文档过大（{}KB，上限 2MB）",
                meta.len() / 1024
            )));
        }
        std::fs::read_to_string(&target).map_err(|e| CoreError::Other(anyhow::anyhow!("{e}")))
    }

    /// 定向编辑 wiki 文档（AI/人工共用）：search/replace 块**全有或全无**——
    /// 每块 search 必须在文中恰好命中一次，任一块不满足即整体失败（零部分应用）。
    /// 适用 cards/ 与 pages/；路径不变不重登记。注意：编辑不触发 RAG 向量化，
    /// 检索刷新需在浏览页「重建向量索引」。
    pub fn edit_wiki_doc(
        &self,
        repo_path: &str,
        doc_path: &str,
        replacements: &[(String, String)],
    ) -> CoreResult<String> {
        if replacements.is_empty() {
            return Err(CoreError::Other(anyhow::anyhow!("替换清单为空")));
        }
        let old = self.read_wiki_doc(repo_path, doc_path, None)?;
        // 先全量校验再应用：失败时文件保持原样（agent 可修 search 重试）
        for (i, (search, _)) in replacements.iter().enumerate() {
            if search.is_empty() {
                return Err(CoreError::Other(anyhow::anyhow!("第 {} 块 search 为空", i + 1)));
            }
            let n = old.matches(search.as_str()).count();
            if n != 1 {
                let head: String = search.chars().take(40).collect();
                return Err(CoreError::Other(anyhow::anyhow!(
                    "第 {} 块 search 命中 {n} 次（要求恰好 1 次）：{head}…",
                    i + 1
                )));
            }
        }
        let mut content = old;
        for (search, replace) in replacements {
            // 校验保证恰好一次，replace 全局替换等价单点替换
            content = content.replace(search.as_str(), replace);
        }
        let (target, _) = self.resolve_wiki_doc(repo_path, doc_path, None)?;
        std::fs::write(&target, &content)?;
        Ok(content)
    }

    /// 删除知识卡（文件 + 登记）
    pub fn delete_card(&self, repo_path: &str, card_rel: &str) -> CoreResult<()> {
        let root = Path::new(repo_path);
        let canon = crate::canonical_key(root)?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Err(CoreError::Other(anyhow::anyhow!("仓库未登记：{repo_path}")));
        };
        // 路径必须落在 cards/ 下
        if !card_rel.starts_with("cards/") || card_rel.contains("..") {
            return Err(CoreError::Other(anyhow::anyhow!("非法卡片路径：{card_rel}")));
        }
        let full = root.join(".bytewiki").join(card_rel);
        if full.exists() {
            std::fs::remove_file(&full)?;
        }
        self.db.delete_wiki_doc(repo.id, card_rel)?;
        Ok(())
    }
}

/// 标题 → slug（保留中英数字与连字符，其余转 -，截断 40 字符）
fn slugify(title: &str) -> String {
    let mut out = String::new();
    for ch in title.chars().take(40) {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => out.push(ch),
            c if c.is_alphanumeric() => out.push(c), // 中文等保留
            _ => out.push('-'),
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::AppService;

    #[test]
    fn create_and_delete_card_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        svc.open_repo(repo.to_str().unwrap()).unwrap();

        let rel = svc.create_card(repo.to_str().unwrap(), "认证要点", "token 存储在 vault").unwrap();
        assert!(rel.starts_with("cards/") && rel.ends_with(".md"));
        let full = repo.join(".bytewiki").join(&rel);
        let body = std::fs::read_to_string(&full).unwrap();
        assert!(body.contains("title: 认证要点") && body.contains("token 存储在 vault"));

        // 登记为 human，且「重新生成」不删除
        let docs = svc.db.list_wiki_docs(1, "main").unwrap();
        assert!(docs.iter().any(|(p, k, t)| p == &rel && k == "card" && t == "认证要点"));
        svc.db
            .replace_wiki_docs(1, "main", &[("architecture.md".into(), "architecture".into(), "x".into())], &now_iso())
            .unwrap();
        let docs = svc.db.list_wiki_docs(1, "main").unwrap();
        assert!(docs.iter().any(|(p, _, _)| p == &rel), "human 卡在生成区替换后保留");

        // 删除
        svc.delete_card(repo.to_str().unwrap(), &rel).unwrap();
        assert!(!full.exists());
        assert!(!svc.db.list_wiki_docs(1, "main").unwrap().iter().any(|(p, _, _)| p == &rel));

        // 非法路径拒绝
        assert!(svc.delete_card(repo.to_str().unwrap(), "modules/a.md").is_err());
    }

    #[test]
    fn empty_title_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        svc.open_repo(repo.to_str().unwrap()).unwrap();
        assert!(svc.create_card(repo.to_str().unwrap(), "  ", "x").is_err());
    }

    /// 定向编辑：全有或全无（AI 编辑的防呆核心语义）
    #[test]
    fn edit_wiki_doc_all_or_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
        svc.open_repo(repo.to_str().unwrap()).unwrap();
        let rel = svc.create_card(repo.to_str().unwrap(), "编辑测试", "第一段。\n\n第二段：旧内容。\n").unwrap();

        // 正常：两块替换全部命中 → 应用
        let out = svc
            .edit_wiki_doc(
                repo.to_str().unwrap(),
                &rel,
                &[
                    ("第一段。".into(), "第一段（已修订）。".into()),
                    ("旧内容".into(), "新内容".into()),
                ],
            )
            .unwrap();
        assert!(out.contains("已修订") && out.contains("新内容"));
        assert!(svc.read_wiki_doc(repo.to_str().unwrap(), &rel, None).unwrap().contains("新内容"));

        // 第二块 search 不存在 → 整体失败，第一块也不应用（零部分写入）
        let err = svc.edit_wiki_doc(
            repo.to_str().unwrap(),
            &rel,
            &[
                ("新内容".into(), "又一次".into()),
                ("根本不存在".into(), "x".into()),
            ],
        );
        assert!(err.is_err());
        let after = svc.read_wiki_doc(repo.to_str().unwrap(), &rel, None).unwrap();
        assert!(after.contains("新内容") && !after.contains("又一次"), "失败时文件保持原样");

        // 多次命中（要求恰好一次）拒绝
        let err2 = svc.edit_wiki_doc(
            repo.to_str().unwrap(),
            &rel,
            &[("。".into(), "！".into())], // 多处句号
        );
        assert!(err2.is_err());

        // 路径穿越拒绝
        assert!(svc
            .edit_wiki_doc(repo.to_str().unwrap(), "cards/../x.md", &[])
            .is_err());
    }
}
