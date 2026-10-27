//! git 常用封装（AI 协作工具面的地基）
//!
//! 供 MCP 工具 / bw CLI / 增量更新管线共用：状态、差异（区间或工作区）、
//! 历史、提交明细。spawn git 子进程（同 git_read.rs 的务实选择），防护同源：
//! rev/分支走保守字符白名单（拒选项注入），路径拒 `..`，输出 2MB 上限 +
//! 二进制检测。全部只读，不产生任何仓库状态变更。

use std::path::Path;
use std::process::Command;

use crate::{CoreError, CoreResult};

const MAX_OUT: usize = 2 * 1024 * 1024;

/// rev / 区间白名单：字母数字与 `. _ / - ^ ~`（覆盖 HEAD、短 sha、main、
/// v1.0、HEAD~3、abc^、a..b）；拒 '-' 开头的选项注入与空格等元字符
fn valid_rev(r: &str) -> bool {
    !r.is_empty()
        && !r.starts_with('-')
        && r.len() <= 128
        && r.chars().all(|c| matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '/' | '.' | '_' | '-' | '^' | '~'))
}

fn valid_path(p: &str) -> bool {
    !p.is_empty() && !p.contains("..") && !p.starts_with('/') && !p.starts_with('\\')
}

fn run_git(root: &Path, args: &[&str]) -> CoreResult<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| CoreError::Other(anyhow::anyhow!("启动 git 失败：{e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(CoreError::Other(anyhow::anyhow!("git {} 失败：{err}", args[0])));
    }
    if out.stdout.len() > MAX_OUT {
        return Err(CoreError::Other(anyhow::anyhow!(
            "输出过大（上限 2MB，可加路径过滤缩小范围）"
        )));
    }
    if out.stdout[..out.stdout.len().min(8192)].contains(&0) {
        return Err(CoreError::Other(anyhow::anyhow!("输出含二进制内容")));
    }
    String::from_utf8(out.stdout).map_err(|_| CoreError::Other(anyhow::anyhow!("非 UTF-8 输出")))
}

/// 工作区状态（porcelain XY 两列 + 路径）
#[derive(Debug, Clone, serde::Serialize)]
pub struct GitFileStatus {
    pub path: String,
    /// 暂存列（A/M/D/R/…）
    pub x: String,
    /// 工作区列
    pub y: String,
}

pub fn status(root: &Path) -> CoreResult<Vec<GitFileStatus>> {
    let out = run_git(root, &["status", "--porcelain"])?;
    Ok(out
        .lines()
        .filter(|l| l.len() >= 4)
        .map(|l| GitFileStatus {
            x: l[0..1].to_string(),
            y: l[1..2].to_string(),
            path: l[3..].trim_matches('"').to_string(),
        })
        .collect())
}

/// 差异文本。
/// - `base = Some(rev)`：`git diff <base>..HEAD [-- path]`（已提交区间）
/// - `base = None`：`git diff HEAD [-- path]`（工作区 vs HEAD，含已暂存）
pub fn diff(root: &Path, base: Option<&str>, path: Option<&str>) -> CoreResult<String> {
    let mut args: Vec<String> = vec!["diff".into()];
    let spec = match base {
        Some(b) => {
            if !valid_rev(b) {
                return Err(CoreError::Other(anyhow::anyhow!("非法 rev：{b}")));
            }
            format!("{b}..HEAD")
        }
        None => "HEAD".to_string(),
    };
    args.push(spec);
    if let Some(p) = path {
        if !valid_path(p) {
            return Err(CoreError::Other(anyhow::anyhow!("非法路径：{p}")));
        }
        args.push("--".into());
        args.push(p.replace('\\', "/"));
    }
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_git(root, &refs)
}

/// 提交历史（最近 n 条，可按路径过滤）
#[derive(Debug, Clone, serde::Serialize)]
pub struct GitLogEntry {
    pub sha: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}

pub fn log(root: &Path, n: usize, path: Option<&str>) -> CoreResult<Vec<GitLogEntry>> {
    let mut args: Vec<String> = vec![
        "log".into(),
        format!("-{}", n.clamp(1, 200)),
        "--pretty=format:%h%x09%an%x09%ad%x09%s".into(),
        "--date=short".into(),
    ];
    if let Some(p) = path {
        if !valid_path(p) {
            return Err(CoreError::Other(anyhow::anyhow!("非法路径：{p}")));
        }
        args.push("--".into());
        args.push(p.replace('\\', "/"));
    }
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = run_git(root, &refs)?;
    Ok(out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let mut it = l.splitn(4, '\t');
            Some(GitLogEntry {
                sha: it.next()?.to_string(),
                author: it.next()?.to_string(),
                date: it.next()?.to_string(),
                subject: it.next()?.to_string(),
            })
        })
        .collect())
}

/// 提交明细（stat + 补丁），供 agent 理解某次提交改了什么
pub fn show(root: &Path, rev: &str, path: Option<&str>) -> CoreResult<String> {
    if !valid_rev(rev) {
        return Err(CoreError::Other(anyhow::anyhow!("非法 rev：{rev}")));
    }
        let mut args: Vec<String> = vec!["show".into(), "--no-textconv".into(), "--stat=200".into(), rev.to_string()];
    if let Some(p) = path {
        if !valid_path(p) {
            return Err(CoreError::Other(anyhow::anyhow!("非法路径：{p}")));
        }
        args.push("--".into());
        args.push(p.replace('\\', "/"));
    }
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_git(root, &refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_ok() -> bool {
        Command::new("git").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
    }

    fn commit_all(dir: &Path, msg: &str) {
        let _ = Command::new("git").arg("-C").arg(dir).args(["add", "."]).output();
        let _ = Command::new("git").arg("-C").arg(dir).args(["commit", "-q", "-m", msg]).output();
    }

    fn init_repo(dir: &Path) {
        assert!(git_ok(), "测试机需要 git");
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "user.name", "t"],
        ] {
            let _ = Command::new("git").arg("-C").arg(dir).args(&args).output();
        }
        std::fs::write(dir.join("app.py"), "def main():\n    return 1\n").unwrap();
        commit_all(dir, "base");
        std::fs::write(dir.join("app.py"), "def main():\n    return 2  # v2\n").unwrap();
        std::fs::write(dir.join("new.txt"), "hello\n").unwrap();
        commit_all(dir, "v2");
    }

    #[test]
    fn status_diff_log_show_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // 工作区脏改
        std::fs::write(dir.path().join("app.py"), "def main():\n    return 3  # dirty\n").unwrap();

        let st = status(dir.path()).unwrap();
        assert_eq!(st.len(), 1, "工作区 1 个脏文件：{st:?}");
        assert_eq!(st[0].path, "app.py");
        assert_eq!(st[0].y, "M");

        // 工作区 vs HEAD：含 dirty 改动
        let d_work = diff(dir.path(), None, None).unwrap();
        assert!(d_work.contains("return 3"), "工作区差异应含 dirty 改动");
        // 区间 base..HEAD（干净）
        let d_range = diff(dir.path(), Some("HEAD~1"), None).unwrap();
        assert!(d_range.contains("return 2"), "区间差异应含 v2 提交");
        // 单路径过滤
        let d_path = diff(dir.path(), Some("HEAD~1"), Some("new.txt")).unwrap();
        assert!(d_path.contains("hello") && !d_path.contains("app.py"), "路径过滤：{d_path}");

        let logs = log(dir.path(), 10, None).unwrap();
        assert!(logs.len() >= 2);
        assert_eq!(logs[0].subject, "v2");
        assert!(logs[0].sha.len() >= 7);

        let sh = show(dir.path(), "HEAD", None).unwrap();
        assert!(sh.contains("v2") && sh.contains("app.py"), "show 应含提交主题与文件");
    }

    #[test]
    fn rejects_injection() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        assert!(diff(dir.path(), Some("--exec=x"), None).is_err(), "rev 选项注入拒绝");
        assert!(diff(dir.path(), Some("a b"), None).is_err(), "空格拒绝");
        assert!(show(dir.path(), "../../etc", None).is_err());
        assert!(diff(dir.path(), None, Some("../x")).is_err(), "路径穿越拒绝");
    }
}
