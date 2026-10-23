//! 按分支读 git 快照文件（章节来源「与文档同分支」的代码视图）
//!
//! 只读查看其他分支的 Wiki/调用链时，引用行号是按**那个分支的代码**生成的；
//! 读工作区实时文件会因分支进度差异严重漂移。这里用 `git show <branch>:<path>`
//! 取分支 tip 的文件内容（手解 packfile 不可行，spawn git 是务实选择——
//! 产品本身就要求 git 存在）。
//!
//! 防护：分支名走保守字符白名单（git ref 规则子集，拒选项注入）；
//! 路径拒 `..` 与绝对路径；体积 4MB 上限 + 二进制检测（与 read_repo_file 同口径）。

use std::path::Path;
use std::process::Command;

use crate::{CoreError, CoreResult};

const MAX_BYTES: usize = 4 * 1024 * 1024;

/// 分支名白名单（git ref 规则的安全子集：拒 '-' 开头的选项注入、':' 等特殊字符）
fn valid_branch(b: &str) -> bool {
    !b.is_empty()
        && !b.starts_with('-')
        && !b.ends_with(".lock")
        && b.chars().all(|c| matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '/' | '.' | '_' | '-'))
}

/// 读某分支 tip 的文件内容（与工作区无关）。
/// 文件不在该分支 / 超限 / 二进制 → 可读错误；git 不可用 → 启动错误。
pub fn show_branch_file(root: &Path, branch: &str, rel_path: &str) -> CoreResult<String> {
    if !valid_branch(branch) {
        return Err(CoreError::Other(anyhow::anyhow!("非法分支名：{branch}")));
    }
    if rel_path.contains("..") || rel_path.starts_with('/') || rel_path.starts_with('\\') {
        return Err(CoreError::Other(anyhow::anyhow!("非法路径：{rel_path}")));
    }
    let rel = rel_path.replace('\\', "/");
    let spec = format!("{branch}:{rel}");
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("show")
        .arg("--no-textconv")
        .arg(&spec)
        .output()
        .map_err(|e| CoreError::Other(anyhow::anyhow!("启动 git 失败（读取分支快照需要 git）：{e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(CoreError::Other(anyhow::anyhow!(
            "分支「{branch}」中不存在 {rel}（该文件可能尚未在此分支提交）{err}"
        )));
    }
    if out.stdout.len() > MAX_BYTES {
        return Err(CoreError::Other(anyhow::anyhow!("文件过大（分支快照读取上限 4MB）：{rel}")));
    }
    if out.stdout[..out.stdout.len().min(8192)].contains(&0) {
        return Err(CoreError::Other(anyhow::anyhow!("二进制文件：{rel}")));
    }
    String::from_utf8(out.stdout)
        .map_err(|_| CoreError::Other(anyhow::anyhow!("非 UTF-8 文本（暂不支持）：{rel}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_ok() -> bool {
        Command::new("git").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
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
        let _ = Command::new("git").arg("-C").arg(dir).args(["add", "."]).output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["commit", "-q", "-m", "base"])
            .output();
    }

    #[test]
    fn reads_branch_snapshot_not_worktree() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // feature 分支改文件并提交；工作区切回 main 再改工作区内容
        let g = |args: &[&str]| Command::new("git").arg("-C").arg(dir.path()).args(args).output();
        let _ = g(&["checkout", "-q", "-b", "feature"]);
        std::fs::write(dir.path().join("app.py"), "def main():\n    return 2  # feature\n").unwrap();
        let _ = g(&["add", "."]);
        let _ = g(&["commit", "-q", "-m", "f"]);
        let _ = g(&["checkout", "-q", "main"]);
        std::fs::write(dir.path().join("app.py"), "WORKTREE DIRTY\n").unwrap();

        let snap = show_branch_file(dir.path(), "feature", "app.py").unwrap();
        assert!(snap.contains("return 2  # feature"), "读到的是 feature 分支快照而非工作区");
        let main_snap = show_branch_file(dir.path(), "main", "app.py").unwrap();
        assert!(main_snap.contains("return 1"), "main 快照不受工作区污染");

        // 不在分支中的文件 / 非法分支名 / 选项注入
        assert!(show_branch_file(dir.path(), "feature", "nope.py").is_err());
        assert!(show_branch_file(dir.path(), "--exec=x", "app.py").is_err());
        assert!(show_branch_file(dir.path(), "feature", "../escape").is_err());
    }
}
