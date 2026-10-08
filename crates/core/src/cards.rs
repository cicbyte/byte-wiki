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
        let canon = root.canonicalize()?.to_string_lossy().into_owned();
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
            .log_change(repo.id, "card", &format!("create cards/{slug}.md"))?;
        Ok(rel)
    }

    /// 删除知识卡（文件 + 登记）
    pub fn delete_card(&self, repo_path: &str, card_rel: &str) -> CoreResult<()> {
        let root = Path::new(repo_path);
        let canon = root.canonicalize()?.to_string_lossy().into_owned();
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
        let docs = svc.db.list_wiki_docs(1).unwrap();
        assert!(docs.iter().any(|(p, k, t)| p == &rel && k == "card" && t == "认证要点"));
        svc.db
            .replace_wiki_docs(1, &[("architecture.md".into(), "architecture".into(), "x".into())], &now_iso())
            .unwrap();
        let docs = svc.db.list_wiki_docs(1).unwrap();
        assert!(docs.iter().any(|(p, _, _)| p == &rel), "human 卡在生成区替换后保留");

        // 删除
        svc.delete_card(repo.to_str().unwrap(), &rel).unwrap();
        assert!(!full.exists());
        assert!(!svc.db.list_wiki_docs(1).unwrap().iter().any(|(p, _, _)| p == &rel));

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
}
