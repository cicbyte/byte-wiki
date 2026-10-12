//! 仓库索引器（FR-02 前置）：文件清单 + 模块结构 + 引用关系 DAG
//!
//! - 遍历：`ignore` crate（遵守 .gitignore）+ 额外排除目录（config 偏好 + 二进制产物）
//! - 模块：一级目录（含源码文件才算），根级文件归 "." 模块
//! - 引用：按语言提取 import/use 语句（正则实现，接口按 extractor 组织，
//!   后续可平滑替换为 tree-sitter 实现），相对路径解析到仓库内目标，
//!   包名/ crate 名映射回仓库内目录，聚合为 module→module 去重边
//!
//! 语言覆盖（PRD M1）：Rust / TypeScript-JavaScript / Python / Go；
//! 其余语言按文件清单收录（lang=other），不做引用提取（优雅降级）。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::service::AppService;
use crate::{CoreError, CoreResult};

/// 单文件索引记录
#[derive(Debug, Clone)]
pub struct FileRecord {
    /// 相对仓库根路径（/ 分隔）
    pub path: String,
    /// rust / ts / py / go / ... / other
    pub lang: &'static str,
    pub size: u64,
    /// 修改时间毫秒（变更检测用）
    pub mtime_ms: i64,
    /// 一级目录名（根级文件为 "."）
    pub module: String,
}

/// 模块间引用边（去重计数）
#[derive(Debug, Clone)]
pub struct EdgeRecord {
    pub from_module: String,
    pub to_module: String,
    pub refs: i64,
}

/// 索引摘要
#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexSummary {
    pub files_indexed: usize,
    pub modules: Vec<ModuleSummary>,
    pub edges: usize,
}

/// 模块摘要（含文件数与语言分布）
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModuleSummary {
    pub name: String,
    pub file_count: usize,
    pub langs: Vec<String>,
}

/// 单文件最大解析体积（超过则只登记不提取引用）
const MAX_PARSE_SIZE: u64 = 1024 * 1024;

/// 额外排除的目录名（构建产物 / 依赖缓存，叠加在 config 偏好之上）
pub(crate) const EXTRA_EXCLUDES: &[&str] = &[
    "node_modules", "target", "dist", "build", "out", ".output", ".next", ".nuxt",
    "__pycache__", ".venv", "venv", "env", ".git", ".idea", ".vscode", ".bytewiki",
    ".repo", "coverage", ".pytest_cache", ".mutants",
];

impl AppService {
    /// 全量索引仓库（替换式）：返回摘要；`on_progress` 每 200 个文件回调一次
    pub fn index_repo(
        &self,
        repo_path: &str,
        on_progress: &(dyn Fn(usize) + Send + Sync),
    ) -> CoreResult<IndexSummary> {
        let root = Path::new(repo_path);
        if !root.join(".git").exists() {
            return Err(CoreError::Other(anyhow::anyhow!("不是 Git 仓库：{repo_path}")));
        }
        let Some(repo) = self.db.get_repo_by_path(&crate::canonical_key(root)?)? else {
            return Err(CoreError::Other(anyhow::anyhow!("仓库未登记，请先打开：{repo_path}")));
        };

        let mut excludes: HashSet<String> = EXTRA_EXCLUDES.iter().map(|s| s.to_string()).collect();
        if let Ok(cfg) = self.config() {
            excludes.extend(cfg.preferences.exclude_dirs);
        }

        let mut files: Vec<FileRecord> = Vec::new();
        let mut imports_by_file: Vec<(String, Vec<String>)> = Vec::new();
        let mut count: usize = 0;

        let walker = ignore::WalkBuilder::new(root)
            .hidden(false)
            .filter_entry(move |e| {
                e.file_name()
                    .to_str()
                    .map(|n| !excludes.contains(n))
                    .unwrap_or(true)
            })
            .build();

        for entry in walker.flatten() {
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let rel = match entry.path().strip_prefix(root) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            let (size, mtime_ms) = entry
                .metadata()
                .map(|m| {
                    let mt = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    (m.len(), mt)
                })
                .unwrap_or((0, 0));
            let lang = lang_of(&rel);
            let module = module_of(&rel);
            files.push(FileRecord { path: rel.clone(), lang, size, mtime_ms, module });

            // 引用提取（源码语言 + 体积限制内）
            if is_source(lang) && size <= MAX_PARSE_SIZE {
                let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
                let imports = extract_imports(lang, &content);
                imports_by_file.push((rel, imports));
            }

            count += 1;
            if count % 200 == 0 {
                on_progress(count);
            }
        }

        // 引用 → 模块边聚合
        let path_set: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();
        let module_of_path: HashMap<String, String> =
            files.iter().map(|f| (f.path.clone(), f.module.clone())).collect();
        // crate/包名 → 模块 映射：一级目录名映射到自身（如 tools → tools），
        // 二级目录名（含 crate 名归一化）映射到所属模块（如 core → crates）；
        // 另解析 Cargo.toml / package.json 的包名（如 byte_wiki_core → crates），
        // 解决 crate 名与目录名不一致时依赖边丢失的问题。
        let mut name_to_module: HashMap<String, String> = HashMap::new();
        for f in &files {
            let segs: Vec<&str> = f.path.split('/').collect();
            if segs.len() >= 2 {
                name_to_module
                    .entry(segs[0].to_string())
                    .or_insert_with(|| segs[0].to_string());
                name_to_module
                    .entry(segs[1].to_string())
                    .or_insert_with(|| segs[0].to_string());
                let norm = segs[1].replace(['-', '_'], "");
                name_to_module.entry(norm).or_insert_with(|| segs[0].to_string());
            }
            if f.size <= MAX_PARSE_SIZE {
                if f.path.ends_with("Cargo.toml") {
                    if let Ok(content) = std::fs::read_to_string(root.join(&f.path)) {
                        if let Some(name) = cargo_pkg_name(&content) {
                            let segs: Vec<&str> = f.path.split('/').collect();
                            let module = segs.first().copied().unwrap_or(".").to_string();
                            name_to_module.entry(name.replace('-', "_")).or_insert(module.clone());
                            name_to_module.entry(name.replace('_', "-")).or_insert(module);
                        }
                    }
                } else if f.path.ends_with("package.json") {
                    if let Ok(content) = std::fs::read_to_string(root.join(&f.path)) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
                                let segs: Vec<&str> = f.path.split('/').collect();
                                let module = segs.first().copied().unwrap_or(".").to_string();
                                name_to_module.entry(name.replace('-', "_")).or_insert(module.clone());
                                name_to_module.entry(name.replace('_', "-")).or_insert(module);
                            }
                        }
                    }
                }
            }
        }

        let mut edge_map: BTreeMap<(String, String), i64> = BTreeMap::new();
        for (from_path, imports) in &imports_by_file {
            let from_mod = match module_of_path.get(from_path) {
                Some(m) => m.clone(),
                None => continue,
            };
            for imp in imports {
                if let Some(to_mod) = resolve_import_to_module(imp, from_path, &path_set, &module_of_path, &name_to_module) {
                    if to_mod != from_mod {
                        *edge_map.entry((from_mod.clone(), to_mod)).or_insert(0) += 1;
                    }
                }
            }
        }
        let edges: Vec<EdgeRecord> = edge_map
            .into_iter()
            .map(|((from_module, to_module), refs)| EdgeRecord { from_module, to_module, refs })
            .collect();

        // 模块摘要
        let mut mod_map: BTreeMap<String, (usize, HashSet<String>)> = BTreeMap::new();
        for f in &files {
            let e = mod_map.entry(f.module.clone()).or_insert((0, HashSet::new()));
            e.0 += 1;
            if is_source(f.lang) {
                e.1.insert(f.lang.to_string());
            }
        }
        let modules: Vec<ModuleSummary> = mod_map
            .into_iter()
            .map(|(name, (file_count, langs))| ModuleSummary {
                name,
                file_count,
                langs: langs.into_iter().collect(),
            })
            .collect();

        let summary = IndexSummary {
            files_indexed: files.len(),
            modules: modules.clone(),
            edges: edges.len(),
        };

        // 入库（替换式）
        let edge_rows: Vec<(String, String, i64)> =
            edges.iter().map(|e| (e.from_module.clone(), e.to_module.clone(), e.refs)).collect();
        let file_rows: Vec<(String, String, i64, String, i64)> = files
            .iter()
            .map(|f| {
                (
                    f.path.clone(),
                    f.lang.to_string(),
                    f.size as i64,
                    f.module.clone(),
                    f.mtime_ms,
                )
            })
            .collect();
        self.db
            .replace_index(repo.id, &file_rows, &edge_rows, &crate::db::now_iso())?;
        self.db.log_change(repo.id, "index", &format!("files={} edges={}", summary.files_indexed, summary.edges))?;

        Ok(summary)
    }
}

/// 按扩展名识别语言
pub(crate) fn lang_of(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "rs" => "rust",
        "ts" | "tsx" | "mts" | "cts" => "ts",
        "js" | "jsx" | "mjs" | "cjs" => "js",
        "py" | "pyi" => "py",
        "go" => "go",
        "md" | "markdown" => "md",
        "json" | "jsonc" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "css" | "scss" | "less" => "css",
        "html" | "vue" | "svelte" => "html",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "sql" => "sql",
        "sh" | "bash" | "ps1" | "bat" => "sh",
        _ => "other",
    }
}

fn is_source(lang: &str) -> bool {
    matches!(lang, "rust" | "ts" | "js" | "py" | "go")
}

/// 一级目录即模块；根级文件归 "."
pub(crate) fn module_of(rel_path: &str) -> String {
    rel_path
        .split('/')
        .next()
        .filter(|s| !rel_path.starts_with('/') && *s != rel_path)
        .unwrap_or(".")
        .to_string()
}

/// 从 Cargo.toml 内容解析 [package] name（首个 name = "..." 即可覆盖大多数场景）
fn cargo_pkg_name(content: &str) -> Option<String> {
    let mut in_package = false;
    for line in content.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_package = l == "[package]";
            continue;
        }
        if in_package && l.starts_with("name") {
            if let Some(v) = l.split('=').nth(1) {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// 提取 import/use 引用（返回原始目标串：相对路径 / 包名 / crate 路径首段）
fn extract_imports(lang: &str, content: &str) -> Vec<String> {
    match lang {
        "rust" => extract_rust(content),
        "ts" | "js" => extract_ts(content),
        "py" => extract_python(content),
        "go" => extract_go(content),
        _ => vec![],
    }
}

fn extract_rust(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("use ") {
            // use a::b::{c, d}; → 取首段 a
            let first = rest.split("::").next().unwrap_or("").trim();
            if !first.is_empty() {
                out.push(first.to_string());
            }
        } else if let Some(rest) = l.strip_prefix("extern crate ") {
            out.push(rest.trim_end_matches(';').trim().to_string());
        }
    }
    out
}

fn extract_ts(content: &str) -> Vec<String> {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r#"(?:from\s+|require\(\s*|import\s*\(\s*)["']([^"']+)["']"#).unwrap()
    });
    re.captures_iter(content)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

fn extract_python(content: &str) -> Vec<String> {
    use std::sync::OnceLock;
    static RE_FROM: OnceLock<regex::Regex> = OnceLock::new();
    static RE_IMPORT: OnceLock<regex::Regex> = OnceLock::new();
    let re_from = RE_FROM.get_or_init(|| regex::Regex::new(r"^\s*from\s+([\w.]+)").unwrap());
    let re_import = RE_IMPORT.get_or_init(|| regex::Regex::new(r"^\s*import\s+([\w.]+)").unwrap());
    let mut out = Vec::new();
    for line in content.lines() {
        if let Some(c) = re_from.captures(line) {
            if let Some(m) = c.get(1) {
                out.push(m.as_str().to_string());
            }
        } else if let Some(c) = re_import.captures(line) {
            if let Some(m) = c.get(1) {
                out.push(m.as_str().to_string());
            }
        }
    }
    out
}

fn extract_go(content: &str) -> Vec<String> {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r#"import\s+(?:\(([^)]*)\)|"([^"]+)")"#).unwrap());
    let mut out = Vec::new();
    for cap in re.captures_iter(content) {
        if let Some(block) = cap.get(1) {
            for line in block.as_str().lines() {
                let l = line.trim();
                if let Some(stripped) = l.strip_prefix('"') {
                    if let Some(path) = stripped.strip_suffix('"') {
                        out.push(path.to_string());
                    }
                }
            }
        } else if let Some(m) = cap.get(2) {
            out.push(m.as_str().to_string());
        }
    }
    out
}

/// 把一条 import 解析到目标模块
fn resolve_import_to_module(
    imp: &str,
    from_path: &str,
    path_set: &HashSet<String>,
    module_of_path: &HashMap<String, String>,
    name_to_module: &HashMap<String, String>,
) -> Option<String> {
    // 1) TS/JS 相对路径：./x ../y → 仓库内文件（尝试后缀与 index）
    if imp.starts_with("./") || imp.starts_with("../") {
        let base_dir = {
            let mut segs: Vec<&str> = from_path.split('/').collect();
            segs.pop();
            segs
        };
        let mut parts: Vec<String> = base_dir.iter().map(|s| s.to_string()).collect();
        for seg in imp.trim_start_matches('.').split('/') {
            match seg {
                "" => {}
                "." => {}
                ".." => {
                    parts.pop();
                }
                s => parts.push(s.to_string()),
            }
        }
        let stem = parts.join("/");
        let candidates = [
            stem.clone(),
            format!("{stem}.ts"),
            format!("{stem}.tsx"),
            format!("{stem}.js"),
            format!("{stem}.vue"),
            format!("{stem}/index.ts"),
            format!("{stem}/index.js"),
        ];
        for c in candidates {
            if path_set.contains(&c) {
                return module_of_path.get(&c).cloned();
            }
        }
        // 解析不到具体文件：按目录层级降级取一级目录
        if let Some(first) = imp.trim_start_matches("./").trim_start_matches("../").split('/').next() {
            if first != "." && first != ".." {
                return module_of_path.get(&format!("{first}/x")).cloned();
            }
        }
        return None;
    }
    // 2) Rust：use crate::/self/super 跳过；crate 名 → name_to_module（含归一化）
    if imp == "crate" || imp == "self" || imp == "super" || imp == "std" {
        return None;
    }
    // 3) Python：点路径首段 → 仓库目录名
    let first_seg = imp.split("::").next().unwrap_or(imp);
    let py_first = first_seg.split('.').next().unwrap_or(first_seg);
    for variant in [
        py_first.to_string(),
        py_first.replace('_', "-"),
        py_first.replace('-', "_"),
    ] {
        if let Some(m) = name_to_module.get(&variant) {
            return Some(m.clone());
        }
    }
    // 4) 归一化（去连字符/下划线）匹配（包名）
    let norm = py_first.replace(['-', '_', '/'], "");
    if !norm.is_empty() {
        if let Some(m) = name_to_module.get(&norm) {
            return Some(m.clone());
        }
    }
    // 5) Go：路径尾部目录名匹配
    if imp.contains('/') {
        let tail = imp.rsplit('/').next().unwrap_or(imp);
        if let Some(m) = name_to_module.get(tail) {
            return Some(m.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests;
