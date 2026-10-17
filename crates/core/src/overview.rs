//! 项目总览聚合：只读 db + outline.json，不扫盘不调 LLM
//!
//! 总览页的数据面——索引（文件/模块/引用边/语言分布）、Wiki（章节/页面）、
//! 向量块数、生成活动历史（change_log 最近记录），一次命令全量带回。

use serde::Serialize;

use crate::service::AppService;
use crate::CoreResult;

/// 总览聚合数据
#[derive(Debug, Clone, Serialize)]
pub struct RepoOverview {
    pub files_indexed: usize,
    pub modules: usize,
    pub edges: usize,
    /// 语言分布（文件数降序，top 8）
    pub langs: Vec<(String, usize)>,
    /// 模块 TOP（文件数降序，top 8）
    pub top_modules: Vec<(String, usize)>,
    /// Wiki 章节数（outline；未生成或无大纲为 0）
    pub wiki_sections: usize,
    /// Wiki 页面数
    pub wiki_pages: usize,
    /// 知识卡数
    pub cards: usize,
    /// 向量分块数
    pub chunks: usize,
    /// 代码规模（按索引清单逐文件数行；跳过大文件与二进制）
    pub total_lines: usize,
    /// 有效行（非空白行）
    pub code_lines: usize,
    pub blank_lines: usize,
    /// 索引文件总体积（字节）
    pub size_bytes: u64,
    /// 按语言行数分布（行数降序 TOP8）：(lang, 文件数, 行数)
    pub lang_lines: Vec<(String, usize, usize)>,
    /// 最近生成/更新活动（时间倒序，top 5）：(kind, detail, created_at)
    pub recent_activity: Vec<(String, String, String)>,
}

impl AppService {
    /// 项目总览聚合（只读；页面进入/生成完成后刷新）
    pub fn repo_overview(&self, repo_path: &str) -> CoreResult<RepoOverview> {
        let canon = crate::canonical_key(std::path::Path::new(repo_path))?;
        let Some(repo) = self.db.get_repo_by_path(&canon)? else {
            return Err(crate::CoreError::Other(anyhow::anyhow!("仓库未登记：{repo_path}")));
        };

        let files = self.db.get_index_files_by_repo(repo.id)?;
        let edges = self.db.get_index_edges_by_repo(repo.id)?;

        // 语言分布 / 模块统计（总数 + TOP）
        let mut lang_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut mod_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (_path, lang, _size, module, _mtime) in &files {
            *lang_count.entry(lang.clone()).or_default() += 1;
            *mod_count.entry(module.clone()).or_default() += 1;
        }
        let module_total = mod_count.len();
        let mut langs: Vec<(String, usize)> = lang_count.into_iter().collect();
        langs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        langs.truncate(8);
        let mut top_modules: Vec<(String, usize)> = mod_count.into_iter().collect();
        top_modules.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        top_modules.truncate(8);

        // Wiki 面：outline 章节/页面 + 文档登记分类
        let (wiki_sections, wiki_pages) = match crate::generator::outline::read_outline(
            &std::path::Path::new(repo_path).join(".bytewiki"),
        ) {
            Some(o) => (o.sections.len(), o.pages.len()),
            None => (0, 0),
        };
        let docs = self.db.list_wiki_docs(repo.id)?;
        let cards = docs.iter().filter(|(_, k, _)| k == "card").count();

        // 代码规模：逐文件数行（本地读盘，几百文件毫秒级；跳过超大与二进制）
        let root = std::path::Path::new(repo_path);
        let mut total_lines = 0usize;
        let mut code_lines = 0usize;
        let mut size_bytes = 0u64;
        let mut lang_lines_map: std::collections::HashMap<String, (usize, usize)> =
            std::collections::HashMap::new(); // lang -> (files, lines)
        const MAX_STAT_SIZE: u64 = 2 * 1024 * 1024;
        for (path, lang, size, _module, _mtime) in &files {
            size_bytes += (*size).max(0) as u64;
            if (*size).max(0) as u64 > MAX_STAT_SIZE {
                if let Some(e) = lang_lines_map.get_mut(lang) {
                    e.0 += 1;
                }
                continue;
            }
            let Ok(content) = std::fs::read_to_string(root.join(path)) else {
                continue; // 二进制/不可读：不计行
            };
            let mut lines = 0usize;
            let mut non_blank = 0usize;
            for line in content.lines() {
                lines += 1;
                if !line.trim().is_empty() {
                    non_blank += 1;
                }
            }
            total_lines += lines;
            code_lines += non_blank;
            let e = lang_lines_map.entry(lang.clone()).or_default();
            e.0 += 1;
            e.1 += lines;
        }
        let blank_lines = total_lines - code_lines;
        let mut lang_lines: Vec<(String, usize, usize)> =
            lang_lines_map.into_iter().map(|(l, (f, n))| (l, f, n)).collect();
        lang_lines.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
        lang_lines.truncate(8);

        Ok(RepoOverview {
            files_indexed: files.len(),
            modules: module_total,
            edges: edges.len(),
            langs,
            top_modules,
            wiki_sections,
            wiki_pages,
            cards,
            chunks: self.db.count_chunks(repo.id)?,
            total_lines,
            code_lines,
            blank_lines,
            size_bytes,
            lang_lines,
            recent_activity: self.db.list_change_log(repo.id, 5)?,
        })
    }
}
