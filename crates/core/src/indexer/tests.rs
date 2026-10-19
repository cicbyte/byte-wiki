//! 索引器测试：临时仓库覆盖多语言、排除策略、模块与边聚合

use super::*;
use crate::service::AppService;

fn svc_in_temp() -> (AppService, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let svc = AppService::open_at(dir.path().join("app"), dir.path().join("data")).unwrap();
    (svc, dir)
}

fn make_repo(dir: &Path) -> std::path::PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();

    // crates/core（rust，引用 byte-wiki-core 之外的 std 与本地 crate）
    std::fs::create_dir_all(repo.join("crates").join("core").join("src")).unwrap();
    std::fs::write(
        repo.join("crates/core/src/lib.rs"),
        "use serde::Serialize;\nuse anyhow::Result;\n",
    )
    .unwrap();
    // apps/desktop（ts，相对引用 apps/shared）
    std::fs::create_dir_all(repo.join("apps").join("desktop").join("src")).unwrap();
    std::fs::create_dir_all(repo.join("apps").join("shared")).unwrap();
    std::fs::write(
        repo.join("apps/desktop/src/main.ts"),
        "import { x } from '../../shared/util';\nimport vue from 'vue';\n",
    )
    .unwrap();
    std::fs::write(repo.join("apps/shared/util.ts"), "export const x = 1;\n").unwrap();
    // scripts（python，引用 tools.helper —— 仓库内目录）
    std::fs::create_dir_all(repo.join("scripts")).unwrap();
    std::fs::create_dir_all(repo.join("tools")).unwrap();
    std::fs::write(repo.join("scripts/run.py"), "import tools.helper\n").unwrap();
    std::fs::write(repo.join("tools/helper.py"), "def f():\n    pass\n").unwrap();
    // 排除项：node_modules 与 .gitignore 命中
    std::fs::create_dir_all(repo.join("node_modules").join("pkg")).unwrap();
    std::fs::write(repo.join("node_modules/pkg/index.js"), "module.exports = 1;\n").unwrap();
    std::fs::write(repo.join(".gitignore"), "gen-out/\n").unwrap();
    std::fs::create_dir_all(repo.join("gen-out")).unwrap();
    std::fs::write(repo.join("gen-out/x.txt"), "x").unwrap();
    // 根级文件 → "." 模块
    std::fs::write(repo.join("README.md"), "# demo\n").unwrap();
    repo
}

#[test]
fn user_exclude_patterns_drop_files_from_index() {
    let (svc, dir) = svc_in_temp();
    let repo = make_repo(dir.path());
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    std::fs::create_dir_all(repo.join("docs").join("deep")).unwrap();
    std::fs::write(repo.join("docs/deep/a.md"), "# a
").unwrap();
    std::fs::write(repo.join("docs/b.md"), "# b
").unwrap();

    // docs/** 排除后：docs 下两个文件不入索引（其余不变）
    svc.set_repo_exclude_patterns(repo.to_str().unwrap(), &["docs/**".to_string()])
        .unwrap();
    let summary = svc.index_repo(repo.to_str().unwrap(), &|_| {}).unwrap();
    assert_eq!(summary.files_indexed, 6, "实际：{:?}", summary.files_indexed);
    let names: Vec<&str> = summary.modules.iter().map(|m| m.name.as_str()).collect();
    assert!(!names.contains(&"docs"), "docs 应被排除：{names:?}");

    // 清空规则恢复收录
    svc.set_repo_exclude_patterns(repo.to_str().unwrap(), &[]).unwrap();
    let summary = svc.index_repo(repo.to_str().unwrap(), &|_| {}).unwrap();
    assert_eq!(summary.files_indexed, 8, "实际：{:?}", summary.files_indexed);
}

#[test]
fn hidden_entries_except_github_are_skipped() {
    let (svc, dir) = svc_in_temp();
    let repo = make_repo(dir.path());
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    // .qoder（工具产物）应排除；.github/workflows 应保留
    std::fs::create_dir_all(repo.join(".qoder").join("content")).unwrap();
    std::fs::write(repo.join(".qoder/content/x.md"), "# x
").unwrap();
    std::fs::create_dir_all(repo.join(".github").join("workflows")).unwrap();
    std::fs::write(repo.join(".github/workflows/ci.yml"), "on: push
").unwrap();

    let summary = svc.index_repo(repo.to_str().unwrap(), &|_| {}).unwrap();
    assert_eq!(summary.files_indexed, 7, "实际：{:?}", summary.files_indexed);
    let names: Vec<&str> = summary.modules.iter().map(|m| m.name.as_str()).collect();
    assert!(!names.contains(&".qoder"), ".qoder 应被排除：{names:?}");
    assert!(names.contains(&".github"), ".github 应保留：{names:?}");
}

#[test]
fn index_extracts_modules_and_edges() {
    let (svc, dir) = svc_in_temp();
    let repo = make_repo(dir.path());
    svc.open_repo(repo.to_str().unwrap()).unwrap();

    let progress_hits: std::sync::Mutex<Vec<usize>> = Default::default();
    let summary = svc
        .index_repo(repo.to_str().unwrap(), &|n| progress_hits.lock().unwrap().push(n))
        .unwrap();

    // 文件清单：源码 5 + README；node_modules / gen-out / 隐藏文件（.gitignore）排除
    assert_eq!(summary.files_indexed, 6, "实际：{:?}", summary.files_indexed);

    // 模块：apps / crates / scripts / tools / "."（gen-out、node_modules 不在）
    let names: Vec<&str> = summary.modules.iter().map(|m| m.name.as_str()).collect();
    for expect in ["apps", "crates", "scripts", "tools", "."] {
        assert!(names.contains(&expect), "缺模块 {expect}：{names:?}");
    }

    // 边：apps→apps（相对引用 shared，同模块内不建边）→ 实际跨模块边为 0？
    // scripts→tools（python import tools.helper）
    let edges = svc.db.get_index_edges_by_repo(1, "main").unwrap();
    let has = |from: &str, to: &str| {
        edges
            .iter()
            .any(|e| e.0 == from && e.1 == to)
    };
    assert!(has("scripts", "tools"), "缺 scripts→tools 边：{edges:?}");
    // rust 引用 serde/anyhow（外部，不映射到仓库目录）→ 不产生边
    // ts 相对引用 apps/shared → apps→apps 同模块，无边
    assert!(summary.edges >= 1);
}

#[test]
fn index_is_replaceable() {
    let (svc, dir) = svc_in_temp();
    let repo = make_repo(dir.path());
    svc.open_repo(repo.to_str().unwrap()).unwrap();
    let s1 = svc.index_repo(repo.to_str().unwrap(), &|_| {}).unwrap();
    // 新增文件后重跑：替换而非追加
    std::fs::write(repo.join("tools/new.py"), "import tools.helper\n").unwrap();
    let s2 = svc.index_repo(repo.to_str().unwrap(), &|_| {}).unwrap();
    assert_eq!(s2.files_indexed, s1.files_indexed + 1);
    let files = svc.db.get_index_files_by_repo(1, "main").unwrap();
    assert_eq!(files.len(), s2.files_indexed);
}

#[test]
fn non_git_repo_rejected() {
    let (svc, dir) = svc_in_temp();
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let err = svc.index_repo(plain.to_str().unwrap(), &|_| {}).unwrap_err();
    assert!(err.to_string().contains("不是 Git 仓库"));
}

#[test]
fn import_extractors_unit() {
    let ts = "import a from './x';\nconst b = require('../y');\nawait import('./z');\n";
    let mut v = extract_ts(ts);
    v.sort();
    assert_eq!(v, vec!["../y".to_string(), "./x".to_string(), "./z".to_string()]);

    let rust = "use serde::Serialize;\nuse crate::foo::Bar;\nextern crate rand;\n";
    assert_eq!(extract_rust(rust), vec!["serde", "crate", "rand"]);

    let py = "import os\nfrom tools.helper import f\nimport a.b as c\n";
    assert_eq!(extract_python(py), vec!["os", "tools.helper", "a.b"]);

    let go = "import \"fmt\"\nimport (\n  \"example.com/x/util\"\n)\n";
    assert_eq!(extract_go(go), vec!["fmt", "example.com/x/util"]);
}

/// 真实仓库冒烟（默认忽略）：对 byte-wiki 自身仓库跑全量索引。
/// `cargo test -p byte-wiki-core index_real_repo -- --ignored --nocapture`
#[test]
#[ignore = "依赖本机真实仓库路径"]
fn index_real_repo() {
    let repo_path = "D:/code/cicbyte/byte-wiki";
    if !Path::new(repo_path).join(".git").exists() {
        eprintln!("跳过：{repo_path} 不是 git 仓库");
        return;
    }
    let (svc, _dir) = svc_in_temp();
    svc.open_repo(repo_path).unwrap();
    let started = std::time::Instant::now();
    let summary = svc.index_repo(repo_path, &|n| eprintln!("  已扫描 {n} 文件")).unwrap();
    eprintln!(
        "索引完成：{} 文件 / {} 模块 / {} 边，耗时 {:?}",
        summary.files_indexed,
        summary.modules.len(),
        summary.edges,
        started.elapsed()
    );
    // 本仓库必然包含的核心模块
    for expect in ["apps", "crates", "tests"] {
        assert!(
            summary.modules.iter().any(|m| m.name == expect),
            "缺模块 {expect}"
        );
    }
    for e in svc.db.get_index_edges_by_repo(1, "main").unwrap() {
        eprintln!("  边：{} → {} (refs={})", e.0, e.1, e.2);
    }
}
