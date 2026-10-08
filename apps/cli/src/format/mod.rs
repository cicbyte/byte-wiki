//! 输出格式化：彩色人类可读 / JSON 双模式
//!
//! 彩色输出使用 ANSI 转义码（Windows 10+ 终端原生支持，Git Bash 也支持）。
//! JSON 输出直接序列化 core 的 model 结构。
//!
//! 各域 `print_*` 拆到子模块；ANSI 颜色常量与 `paint`/`truncate`/`pad`/`format_size`
//! 工具定义在本模块，供子模块 `use super::{...}` 复用；`print_*` 经 `pub use` 重导出，
//! 使 `cmd` 各 `run_*` 里的 `format::print_xxx` 调用路径保持不变。

mod misc;
mod note;
mod project;
mod snippet;
mod template;

pub use misc::{print_categories, print_share, print_share_list};
pub use note::{print_note, print_note_search, print_notes};
pub use project::{
    print_file_tree, print_lang_stats, print_project, print_projects, print_search, print_stats,
    print_tag_stats,
};
pub use snippet::{print_snippet, print_snippet_search, print_snippets};
pub use template::{print_template, print_template_search, print_templates};

// ===== ANSI 颜色（子模块复用）=====

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const BLUE: &str = "\x1b[34m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const MAGENTA: &str = "\x1b[35m";
const GRAY: &str = "\x1b[90m";

/// 给字符串上色（color + 内容 + RESET）
fn paint(color: &str, s: &str) -> String {
    format!("{color}{s}{RESET}")
}

// ===== 小工具（子模块复用）=====

fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let mut t: String = chars[..max.saturating_sub(1)].iter().collect();
        t.push('…');
        t
    }
}

fn pad(s: &str, width: usize) -> String {
    let display_width = s.chars().count();
    if display_width >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - display_width))
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
