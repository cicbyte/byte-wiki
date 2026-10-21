//! 配置数据类型（config.toml）

use serde::{Deserialize, Serialize};

/// 用户配置（config.toml，固定位于 app_root/config/）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 数据目录路径（None = 默认 app_root/data/）
    #[serde(default)]
    pub data_dir: Option<String>,
    #[serde(default)]
    pub backup: BackupConfig,
    #[serde(default)]
    pub api: ApiConfig,
    /// 模型 Provider（生成 / 问答共用）
    #[serde(default)]
    pub llm: LlmConfig,
    /// 用户偏好（索引排除目录等）
    #[serde(default)]
    pub preferences: PreferencesConfig,
}

/// 模型供应商（参考 byte-voice：多供应商列表，单个生效）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProvider {
    /// 唯一 id（稳定标识，配置引用用）
    pub id: String,
    /// 显示名（如「Ollama 本地」「DeepSeek」）
    pub name: String,
    /// 协议类型：ollama / openai（OpenAI 兼容端点）
    #[serde(default = "default_provider_kind")]
    pub kind: String,
    /// API 端点（ollama 默认 http://localhost:11434；openai 兼容如 https://host/v1）
    #[serde(default = "default_ollama_endpoint")]
    pub api_endpoint: String,
    /// API 密钥（ollama 留空）
    #[serde(default)]
    pub api_key: String,
    /// 是否启用（禁用后不可被设为当前使用）
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 可用模型列表（手动添加或「获取模型」拉取）
    #[serde(default)]
    pub models: Vec<String>,
    /// 默认对话/生成模型
    #[serde(default)]
    pub chat_model: String,
    /// 默认 embedding 模型（RAG 向量化）
    #[serde(default)]
    pub embed_model: String,
}

/// 模型 Provider 配置（FR-09：多供应商，本地 Ollama 与远程 OpenAI 兼容可切换）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// 供应商列表（预置见 [`preset_providers`]）
    #[serde(default = "default_providers")]
    pub providers: Vec<LlmProvider>,
    /// 当前使用的供应商 id
    #[serde(default = "default_active_id")]
    pub active_id: String,
    /// Embedding 独立供应商 id（None/空 = 跟随当前供应商）——
    /// 支持「对话用远程 + 向量化用本地」组合；切换后旧向量空间失效需重建
    #[serde(default)]
    pub embed_provider_id: Option<String>,
    /// 预置供应商是否已补齐过（true 后用户删除的预置不再自动复活）
    #[serde(default)]
    pub presets_seeded: bool,
}

impl LlmConfig {
    /// 当前使用的供应商（active_id 命中）
    pub fn active(&self) -> Option<&LlmProvider> {
        self.providers.iter().find(|p| p.id == self.active_id)
    }
}

fn default_provider_kind() -> String {
    "ollama".into()
}

fn default_ollama_endpoint() -> String {
    "http://localhost:11434".into()
}

/// 内置预置供应商定义（与 byte-voice 的数据库预置对齐 + OpenAI 兼容自定义模板）
pub fn preset_providers() -> Vec<LlmProvider> {
    vec![
        LlmProvider {
            id: "ollama-local".into(),
            name: "Ollama 本地".into(),
            kind: "ollama".into(),
            api_endpoint: default_ollama_endpoint(),
            api_key: String::new(),
            enabled: true,
            models: vec![],
            chat_model: default_chat_model(),
            embed_model: default_embed_model(),
        },
        LlmProvider {
            id: "cherry-studio".into(),
            name: "Cherry Studio".into(),
            kind: "openai".into(),
            api_endpoint: "http://127.0.0.1:23333/v1".into(),
            api_key: String::new(),
            enabled: true,
            models: vec!["LongCat-Flash-Chat".into()],
            chat_model: "LongCat-Flash-Chat".into(),
            embed_model: String::new(),
        },
        LlmProvider {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            kind: "openai".into(),
            api_endpoint: "https://api.deepseek.com".into(),
            api_key: String::new(),
            enabled: false,
            models: vec!["deepseek-chat".into(), "deepseek-reasoner".into()],
            chat_model: "deepseek-chat".into(),
            embed_model: String::new(),
        },
        LlmProvider {
            id: "mimo".into(),
            name: "MiMo".into(),
            kind: "openai".into(),
            api_endpoint: "https://api.xiaomimimo.com/v1".into(),
            api_key: String::new(),
            enabled: false,
            models: vec![],
            chat_model: String::new(),
            embed_model: String::new(),
        },
        LlmProvider {
            id: "openai-compat".into(),
            name: "OpenAI 兼容".into(),
            kind: "openai".into(),
            api_endpoint: String::new(),
            api_key: String::new(),
            enabled: false,
            models: vec![],
            chat_model: String::new(),
            embed_model: String::new(),
        },
    ]
}

fn default_providers() -> Vec<LlmProvider> {
    preset_providers()
}

fn default_active_id() -> String {
    "ollama-local".into()
}

fn default_chat_model() -> String {
    // PRD 风险章节的默认建议（用户可在设置页更换为本机已有模型）
    "qwen3-coder:30b".into()
}

fn default_embed_model() -> String {
    "nomic-embed-text".into()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            providers: default_providers(),
            active_id: default_active_id(),
            embed_provider_id: None,
            presets_seeded: false,
        }
    }
}

/// 备份相关配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    /// 默认导出路径（None = 用 data_root/backups/）
    #[serde(default)]
    pub output_dir: Option<String>,
    /// 导出时是否压缩（true=zip，false=目录快照）
    #[serde(default = "default_true")]
    pub compress: bool,
}

fn default_true() -> bool {
    true
}

/// API 服务相关配置（REST API + MCP 各有独立 Key）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// REST API Key（认证用，首次启动自动生成）
    #[serde(default)]
    pub key: String,
    /// MCP HTTP 服务 Key（独立于 REST API）
    #[serde(default)]
    pub mcp_key: String,
    /// 本地访问（127.0.0.1）是否跳过认证
    #[serde(default = "default_true")]
    pub allow_local_no_key: bool,
}

/// 用户偏好配置（导入行为等）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferencesConfig {
    /// 导入项目时排除的目录名（按名精确匹配，递归跳过整个子树）。
    /// 默认预置常见依赖/构建/IDE 目录。
    #[serde(default = "default_exclude_dirs")]
    pub exclude_dirs: Vec<String>,
    /// 大纲阶段使用 agent 多轮自主探查（读文件/AST 符号/搜索后再出纲；
    /// 失败自动回退单发大纲）。默认开。
    #[serde(default = "default_true")]
    pub outline_agent: bool,
    /// 调用链规则（入口噪声治理与自定义识别；规则页管理）
    #[serde(default)]
    pub callchain: CallchainRules,
}

/// 调用链规则：文件收录与入口判定的用户可配面
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallchainRules {
    /// 不收录进调用图的文件 glob（仓库相对路径，如 "vendor/**"）
    #[serde(default)]
    pub exclude_globs: Vec<String>,
    /// 仍收录但不标入口的文件 glob——治「build.rs/examples/benches/tests
    /// 里一堆 main/装饰器命令」的入口噪声
    #[serde(default = "default_entry_excludes")]
    pub entry_exclude_globs: Vec<String>,
    /// 自定义入口 qualname 正则（叠加内置识别，如 "^(run|start)_")
    #[serde(default)]
    pub entry_regex: Option<String>,
}

impl Default for CallchainRules {
    fn default() -> Self {
        Self {
            exclude_globs: vec![],
            entry_exclude_globs: default_entry_excludes(),
            entry_regex: None,
        }
    }
}

fn default_entry_excludes() -> Vec<String> {
    vec![
        "build.rs".into(),
        "examples/**".into(),
        "benches/**".into(),
        "tests/**".into(),
        "test/**".into(),
    ]
}

/// 预置的默认排除目录（常见依赖、构建产物、IDE 配置、系统垃圾文件）
fn default_exclude_dirs() -> Vec<String> {
    vec![
        "node_modules".into(),
        ".git".into(),
        "target".into(),
        "dist".into(),
        "build".into(),
        ".next".into(),
        ".nuxt".into(),
        "out".into(),
        ".output".into(),
        "__pycache__".into(),
        ".venv".into(),
        "venv".into(),
        ".idea".into(),
        ".vscode".into(),
        ".DS_Store".into(),
    ]
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            data_dir: None,
            backup: BackupConfig::default(),
            api: ApiConfig::default(),
            llm: LlmConfig::default(),
            preferences: PreferencesConfig::default(),
        }
    }
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            output_dir: None,
            compress: true,
        }
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            key: String::new(),
            mcp_key: String::new(),
            allow_local_no_key: true,
        }
    }
}

impl Default for PreferencesConfig {
    fn default() -> Self {
        Self {
            exclude_dirs: default_exclude_dirs(),
            outline_agent: true,
            callchain: CallchainRules::default(),
        }
    }
}

#[cfg(test)]
mod preset_tests {
    use super::*;

    #[test]
    fn presets_cover_required_providers() {
        let presets = preset_providers();
        let ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
        for expect in ["ollama-local", "cherry-studio", "deepseek", "mimo", "openai-compat"] {
            assert!(ids.contains(&expect), "缺预置 {expect}");
        }
        // Cherry Studio 端点与默认模型对齐 byte-voice
        let cherry = presets.iter().find(|p| p.id == "cherry-studio").unwrap();
        assert_eq!(cherry.api_endpoint, "http://127.0.0.1:23333/v1");
        assert_eq!(cherry.chat_model, "LongCat-Flash-Chat");
        // DeepSeek 预置模型
        let ds = presets.iter().find(|p| p.id == "deepseek").unwrap();
        assert!(ds.models.iter().any(|m| m == "deepseek-reasoner"));
    }
}
