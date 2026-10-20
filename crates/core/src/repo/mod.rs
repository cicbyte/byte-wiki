//! 仓库域业务：打开（校验 Git 仓库）、最近列表
//!
//! Git 识别采用轻量探测（`.git` 存在性 + HEAD 解析），满足 M1 的
//! 校验/分支名需求；M2 变更增量（diff/log）再引入完整 Git 库。

use std::path::PathBuf;
use std::path::Path;

use crate::db::RepoInfo;
use crate::{CoreError, CoreResult};
use crate::service::AppService;

/// 快速文件计数上限（超出记 10000；精确计数由索引器负责）
const FILE_COUNT_CAP: i64 = 10_000;

/// 读取仓库 HEAD 分支名（普通仓库解析 .git/HEAD 的 ref: 行）
fn head_branch(repo_root: &Path) -> Option<String> {
    let head = std::fs::read_to_string(repo_root.join(".git").join("HEAD")).ok()?;
    head.strip_prefix("ref: refs/heads/")
        .map(|s| s.trim().to_string())
}

/// 解析后的 Git URL 结构（归一化后）
pub struct GitUrl {
    /// 域名（含端口，若有）小写，如 github.com / git.example.cn:8443
    pub host_with_port: String,
    /// 平台目录名：域名去 www./端口/常见企业后缀（.com/.cn/.io/...），路径安全化
    pub platform: String,
    /// 用户/组织名
    pub username: String,
    /// 仓库名（去 .git 后缀）
    pub repo: String,
    /// 原始路径段（host 后的部分，用于还原克隆 URL）
    pub path_raw: String,
}

/// 域名后缀剥离表（平台目录名提取用）
const DOMAIN_SUFFIXES: &[&str] = &[
    ".com", ".cn", ".net", ".org", ".io", ".dev", ".ai", ".co", ".vip", ".site", ".tech",
];

/// 路径段安全化：非字母数字与 - _ . 一律转 '-'，连续压缩
fn sanitize_seg(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// 归一化 Git URL：无 scheme 自动补 https://；校验 http(s)、无空白；
/// 解析出 平台/用户/仓库名 三段目录结构。纯函数（便于单测）。
pub fn normalize_git_url(input: &str) -> CoreResult<GitUrl> {
    let raw = input.trim();
    if raw.is_empty() || raw.contains(char::is_whitespace) {
        return Err(CoreError::Other(anyhow::anyhow!("URL 为空或含空白字符")));
    }
    // 无 scheme 视为 https（用户常直接粘贴 github.com/user/repo）
    let has_scheme = raw.starts_with("https://") || raw.starts_with("http://");
    let full = if has_scheme {
        raw.to_string()
    } else {
        format!("https://{raw}")
    };
    let rest = full
        .strip_prefix("https://")
        .or_else(|| full.strip_prefix("http://"))
        .unwrap_or(&full);
    if rest.is_empty() {
        return Err(CoreError::Other(anyhow::anyhow!("URL 缺少主机部分：{raw}")));
    }
    let (host, path) = match rest.split_once('/') {
        Some((h, p)) => (h, p),
        None => (rest, ""),
    };
    let host = host.trim_end_matches('.').to_lowercase();
    if host.is_empty() || !host.contains('.') {
        return Err(CoreError::Other(anyhow::anyhow!("主机名不合法（需完整域名）：{raw}")));
    }
    // 平台名：去 www. / 端口 / 常见后缀
    let host_no_port = host.split(':').next().unwrap_or(&host);
    let bare = host_no_port.strip_prefix("www.").unwrap_or(host_no_port);
    let mut platform = bare.to_string();
    for suf in DOMAIN_SUFFIXES {
        if let Some(stripped) = platform.strip_suffix(suf) {
            // 仅当剥离后仍非空且不为空段（git.example.cn → git.example）
            if !stripped.is_empty() {
                platform = stripped.to_string();
            }
            break;
        }
    }
    let platform = sanitize_seg(&platform);
    if platform.is_empty() {
        return Err(CoreError::Other(anyhow::anyhow!("无法从域名提取平台名：{raw}")));
    }
    // 路径段：至少 用户/仓库 两段
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 {
        return Err(CoreError::Other(anyhow::anyhow!(
            "URL 需包含 <用户名>/<仓库名> 两段：{raw}"
        )));
    }
    let username = sanitize_seg(segs[0]);
    let repo = sanitize_seg(segs[segs.len() - 1].trim_end_matches(".git"));
    if username.is_empty() || repo.is_empty() {
        return Err(CoreError::Other(anyhow::anyhow!("用户名/仓库名解析为空：{raw}")));
    }
    Ok(GitUrl {
        host_with_port: host,
        platform,
        username,
        repo,
        path_raw: segs.join("/"),
    })
}

/// 共享仓库托管根：~/.cicbyte/share/repos（跨 app 共享，独立于 data_dir）
fn shared_repos_root() -> CoreResult<PathBuf> {
    Ok(crate::service::dirs_or_home()?.join(".cicbyte").join("share").join("repos"))
}

impl AppService {
    /// 打开仓库：校验为 Git 仓库 → 快速统计 → 登记入库（刷新 last_opened_at）
    /// 克隆远程 Git 仓库到共享托管目录
    /// `~/.cicbyte/share/repos/<平台>/<用户>/<仓库名>` 并按本地仓库登记——
    /// 后续索引/生成/分支隔离全走现有管线。仅 http(s)；git 独立参数调用
    /// （无 shell，无注入面）。
    #[allow(clippy::result_large_err)]
    pub fn clone_git_repo(&self, url: &str) -> CoreResult<RepoInfo> {
        let u = normalize_git_url(url)?;
        let target = shared_repos_root()?.join(&u.platform).join(&u.username).join(&u.repo);
        if target.exists() {
            return Err(CoreError::Other(anyhow::anyhow!(
                "目录已存在：{}
如需重新克隆请先删除该目录（或在其中直接「打开项目」复用）",
                target.display()
            )));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let full = format!("https://{}/{}", u.host_with_port, u.path_raw);
        let out = std::process::Command::new("git")
            .arg("clone")
            .arg(&full)
            .arg(&target)
            .output()
            .map_err(|e| CoreError::Other(anyhow::anyhow!("git 执行失败（本机需安装 git）：{e}")))?;
        if !out.status.success() {
            let _ = std::fs::remove_dir_all(&target); // 清理半成品
            return Err(CoreError::Other(anyhow::anyhow!(
                "克隆失败：{}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let info = self.open_repo(&target.to_string_lossy())?;
        self.db.set_repo_origin_url(&info.path, &full)?;
        Ok(info)
    }

    pub fn open_repo(&self, path: &str) -> CoreResult<RepoInfo> {
        let root = Path::new(path);
        if !root.is_dir() {
            return Err(CoreError::Other(anyhow::anyhow!(
                "路径不存在或不是目录：{path}"
            )));
        }
        if !root.join(".git").exists() {
            return Err(CoreError::Other(anyhow::anyhow!(
                "不是 Git 仓库（未找到 .git）：{path}。请选择一个已 git init / clone 的仓库。"
            )));
        }
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        let branch = head_branch(root);
        // 旧版单副本布局 → 分支目录（幂等；.bytewiki/cards 保持共享位）
        crate::branch::migrate_legacy_wiki(root, branch.as_deref().unwrap_or(""))?;
        let file_count = quick_file_count(root);
        self.db.upsert_repo(
            &crate::canonical_key(root)?,
            &name,
            branch.as_deref(),
            file_count,
        )
    }

    /// 最近打开的仓库列表
    pub fn recent_repos(&self, limit: i64) -> CoreResult<Vec<RepoInfo>> {
        self.db.list_recent_repos(limit)
    }
}

/// 快速统计仓库文件数（遵守 .gitignore，排除 .git），封顶 FILE_COUNT_CAP
fn quick_file_count(root: &Path) -> i64 {
    use ignore::WalkBuilder;
    let mut count: i64 = 0;
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|e| e.file_name() != ".git" && crate::indexer::keep_entry(e))
        .build();
    for entry in walker.flatten() {
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            count += 1;
            if count >= FILE_COUNT_CAP {
                return FILE_COUNT_CAP;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc_in_temp() -> (AppService, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let app_root = dir.path().join("app");
        let data_dir = dir.path().join("data");
        let svc = AppService::open_at(&app_root, &data_dir).unwrap();
        (svc, dir)
    }

    /// 造一个最小 git 仓库（仅 .git/HEAD，足够探测）
    fn make_git_repo(parent: &Path, name: &str) -> std::path::PathBuf {
        let repo = parent.join(name);
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(repo.join("a.txt"), "hello").unwrap();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src").join("b.rs"), "fn main() {}").unwrap();
        repo
    }

    // ============ normalize_git_url 解析矩阵 ============

    fn parts(u: &str) -> (String, String, String) {
        let g = normalize_git_url(u).unwrap();
        (g.platform.clone(), g.username.clone(), g.repo.clone())
    }

    #[test]
    #[test]
    fn parse_common_platforms() {
        assert_eq!(parts("https://github.com/user/repo"), ("github".to_string(), "user".to_string(), "repo".to_string()));
        assert_eq!(parts("https://gitee.com/mirrors/toml"), ("gitee".to_string(), "mirrors".to_string(), "toml".to_string()));
        assert_eq!(parts("https://gitlab.com/grp/proj"), ("gitlab".to_string(), "grp".to_string(), "proj".to_string()));
        // bitbucket.org → 去 .org
        assert_eq!(parts("https://bitbucket.org/a/b"), ("bitbucket".to_string(), "a".to_string(), "b".to_string()));
    }

    #[test]
    fn parse_suffixes_slashes_and_scheme_less() {
        // .git 后缀
        assert_eq!(parts("https://github.com/u/r.git"), ("github".to_string(), "u".to_string(), "r".to_string()));
        // 尾斜杠 + .git 组合
        assert_eq!(parts("https://github.com/u/r.git/"), ("github".to_string(), "u".to_string(), "r".to_string()));
        // 无 scheme 自动补 https
        assert_eq!(parts("github.com/u/r"), ("github".to_string(), "u".to_string(), "r".to_string()));
        // http 同样接受
        assert_eq!(parts("http://github.com/u/r"), ("github".to_string(), "u".to_string(), "r".to_string()));
        // 大写域名归一化
        assert_eq!(parts("https://GITHUB.COM/u/r"), ("github".to_string(), "u".to_string(), "r".to_string()));
    }

    #[test]
    fn parse_custom_domains_and_ports() {
        // 自建域名去 .cn；子域保留
        assert_eq!(parts("https://git.example.cn/team/svc"), ("git.example".to_string(), "team".to_string(), "svc".to_string()));
        // .io 后缀
        assert_eq!(parts("https://git.thing.io/a/b"), ("git.thing".to_string(), "a".to_string(), "b".to_string()));
        // 端口去除、不影响平台名
        assert_eq!(parts("https://gitee.com:8443/u/r"), ("gitee".to_string(), "u".to_string(), "r".to_string()));
        // www. 前缀去除
        assert_eq!(parts("https://www.gitee.com/u/r"), ("gitee".to_string(), "u".to_string(), "r".to_string()));
        // 域名特殊字符安全化（转 -）
        let g = normalize_git_url("https://git_ex.io/u/r").unwrap();
        assert_eq!(g.platform, "git-ex");
    }

    #[test]
    fn parse_rejects_bad_inputs() {
        // 只有一段（缺仓库名）
        assert!(normalize_git_url("https://github.com/onlyuser").is_err());
        // 非域名主机
        assert!(normalize_git_url("https://localhost/u/r").is_err());
        // 含空白
        assert!(normalize_git_url("https://github.com/u /r").is_err());
        // 空串
        assert!(normalize_git_url("   ").is_err());
        // ssh 协议拒绝（scheme 非 http(s) 不剥离 → 主机校验失败）
        assert!(normalize_git_url("git@github.com:u/r.git").is_err());
        // 非法协议
        assert!(normalize_git_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn clone_target_layout_is_platform_user_repo() {
        // 结构化路径拼接正确（不实际克隆）
        let g = normalize_git_url("https://gitee.com/mirrors/toml.git").unwrap();
        let root = shared_repos_root().unwrap();
        let expect = root.join("gitee").join("mirrors").join("toml");
        let got = root.join(&g.platform).join(&g.username).join(&g.repo);
        assert_eq!(got, expect);
        let s = got.to_string_lossy();
        assert!(s.contains(".cicbyte") && s.contains("share") && s.contains("repos"));
    }

    #[test]
    fn open_repo_registers_and_rejects_non_git() {
        let (svc, dir) = svc_in_temp();
        let repo = make_git_repo(dir.path(), "demo");

        let info = svc.open_repo(repo.to_str().unwrap()).unwrap();
        assert_eq!(info.name, "demo");
        assert_eq!(info.branch.as_deref(), Some("main"));
        // a.txt + src/b.rs（.git 被 filter_entry 排除）
        assert_eq!(info.file_count, 2);

        // 非 git 目录被拒
        let plain = dir.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let err = svc.open_repo(plain.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("不是 Git 仓库"));

        // 不存在的路径被拒
        let err = svc.open_repo(dir.path().join("nope").to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("路径不存在"));

        // 最近列表可取回
        let recent = svc.recent_repos(5).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].name, "demo");
    }

    #[test]
    fn gitignore_respected_in_count() {
        let (svc, dir) = svc_in_temp();
        let repo = make_git_repo(dir.path(), "demo2");
        std::fs::write(repo.join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(repo.join("ignored.txt"), "x").unwrap();
        let info = svc.open_repo(repo.to_str().unwrap()).unwrap();
        // a.txt + src/b.rs；ignored.txt 被 gitignore 排除，.gitignore 属隐藏文件不计
        assert_eq!(info.file_count, 2);
    }
}
