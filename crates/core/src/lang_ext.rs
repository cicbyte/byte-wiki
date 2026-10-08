//! 语言名 → 文件扩展名映射
//!
//! 用于代码片段落盘时确定文件后缀（rust→rs，python→py…）。
//! 与 cli detect_lang 反向（detect_lang 是 ext→lang）。

/// 把语言名映射为文件扩展名（不含点）。未知语言返回 "txt"。
pub fn lang_to_ext(lang: &str) -> &str {
    match lang.to_ascii_lowercase().as_str() {
        "rust" | "rs" => "rs",
        "go" => "go",
        "python" | "py" => "py",
        "javascript" | "js" => "js",
        "typescript" | "ts" => "ts",
        "vue" => "vue",
        "java" => "java",
        "kotlin" | "kt" => "kt",
        "swift" => "swift",
        "c" => "c",
        "cpp" | "c++" | "cc" => "cpp",
        "csharp" | "cs" | "c#" => "cs",
        "ruby" | "rb" => "rb",
        "php" => "php",
        "shell" | "sh" | "bash" | "zsh" => "sh",
        "sql" => "sql",
        "html" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "markdown" | "md" => "md",
        "lua" => "lua",
        "r" => "r",
        "scala" => "scala",
        "dart" => "dart",
        "perl" => "pl",
        "haskell" | "hs" => "hs",
        "elixir" | "ex" | "exs" => "ex",
        "clojure" | "clj" => "clj",
        "dockerfile" => "dockerfile",
        "makefile" | "make" => "mk",
        "graphql" | "gql" => "graphql",
        "proto" | "protobuf" => "proto",
        "json5" => "json5",
        "ini" => "ini",
        "bat" | "batch" => "bat",
        "powershell" | "ps1" => "ps1",
        "text" | "txt" | "" => "txt",
        _ => "txt",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_langs() {
        assert_eq!(lang_to_ext("rust"), "rs");
        assert_eq!(lang_to_ext("Python"), "py");
        assert_eq!(lang_to_ext("TypeScript"), "ts");
    }

    #[test]
    fn unknown_falls_back_txt() {
        assert_eq!(lang_to_ext("brainfuck"), "txt");
        assert_eq!(lang_to_ext(""), "txt");
    }

    // ============ P3：覆盖全部语言映射分支 ============

    #[test]
    fn lang_to_ext_all_mappings() {
        // 主流语言
        assert_eq!(lang_to_ext("rust"), "rs");
        assert_eq!(lang_to_ext("rs"), "rs");
        assert_eq!(lang_to_ext("go"), "go");
        assert_eq!(lang_to_ext("python"), "py");
        assert_eq!(lang_to_ext("py"), "py");
        assert_eq!(lang_to_ext("javascript"), "js");
        assert_eq!(lang_to_ext("js"), "js");
        assert_eq!(lang_to_ext("typescript"), "ts");
        assert_eq!(lang_to_ext("ts"), "ts");
        assert_eq!(lang_to_ext("vue"), "vue");
        assert_eq!(lang_to_ext("java"), "java");
        assert_eq!(lang_to_ext("kotlin"), "kt");
        assert_eq!(lang_to_ext("kt"), "kt");
        assert_eq!(lang_to_ext("swift"), "swift");
        // C 系列
        assert_eq!(lang_to_ext("c"), "c");
        assert_eq!(lang_to_ext("cpp"), "cpp");
        assert_eq!(lang_to_ext("c++"), "cpp");
        assert_eq!(lang_to_ext("cc"), "cpp");
        assert_eq!(lang_to_ext("csharp"), "cs");
        assert_eq!(lang_to_ext("cs"), "cs");
        assert_eq!(lang_to_ext("c#"), "cs");
        // 脚本语言
        assert_eq!(lang_to_ext("ruby"), "rb");
        assert_eq!(lang_to_ext("rb"), "rb");
        assert_eq!(lang_to_ext("php"), "php");
        assert_eq!(lang_to_ext("shell"), "sh");
        assert_eq!(lang_to_ext("bash"), "sh");
        assert_eq!(lang_to_ext("zsh"), "sh");
        assert_eq!(lang_to_ext("lua"), "lua");
        assert_eq!(lang_to_ext("perl"), "pl");
        // 数据/配置
        assert_eq!(lang_to_ext("sql"), "sql");
        assert_eq!(lang_to_ext("json"), "json");
        assert_eq!(lang_to_ext("json5"), "json5");
        assert_eq!(lang_to_ext("yaml"), "yaml");
        assert_eq!(lang_to_ext("yml"), "yaml");
        assert_eq!(lang_to_ext("toml"), "toml");
        assert_eq!(lang_to_ext("xml"), "xml");
        assert_eq!(lang_to_ext("ini"), "ini");
        assert_eq!(lang_to_ext("graphql"), "graphql");
        assert_eq!(lang_to_ext("gql"), "graphql");
        assert_eq!(lang_to_ext("proto"), "proto");
        assert_eq!(lang_to_ext("protobuf"), "proto");
        // 标记/样式
        assert_eq!(lang_to_ext("html"), "html");
        assert_eq!(lang_to_ext("css"), "css");
        assert_eq!(lang_to_ext("scss"), "scss");
        assert_eq!(lang_to_ext("sass"), "scss");
        assert_eq!(lang_to_ext("markdown"), "md");
        assert_eq!(lang_to_ext("md"), "md");
        // 其他
        assert_eq!(lang_to_ext("r"), "r");
        assert_eq!(lang_to_ext("scala"), "scala");
        assert_eq!(lang_to_ext("dart"), "dart");
        assert_eq!(lang_to_ext("haskell"), "hs");
        assert_eq!(lang_to_ext("hs"), "hs");
        assert_eq!(lang_to_ext("elixir"), "ex");
        assert_eq!(lang_to_ext("ex"), "ex");
        assert_eq!(lang_to_ext("exs"), "ex");
        assert_eq!(lang_to_ext("clojure"), "clj");
        assert_eq!(lang_to_ext("clj"), "clj");
        assert_eq!(lang_to_ext("dockerfile"), "dockerfile");
        assert_eq!(lang_to_ext("makefile"), "mk");
        assert_eq!(lang_to_ext("make"), "mk");
        assert_eq!(lang_to_ext("bat"), "bat");
        assert_eq!(lang_to_ext("batch"), "bat");
        assert_eq!(lang_to_ext("powershell"), "ps1");
        assert_eq!(lang_to_ext("ps1"), "ps1");
        assert_eq!(lang_to_ext("text"), "txt");
        assert_eq!(lang_to_ext("txt"), "txt");
    }

    #[test]
    fn lang_to_ext_case_insensitive() {
        // 大小写不敏感
        assert_eq!(lang_to_ext("RUST"), "rs");
        assert_eq!(lang_to_ext("Rust"), "rs");
        assert_eq!(lang_to_ext("PYTHON"), "py");
        assert_eq!(lang_to_ext("JavaScript"), "js");
    }
}
