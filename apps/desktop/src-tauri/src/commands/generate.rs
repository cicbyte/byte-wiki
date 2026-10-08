//! Wiki 生成命令：一键生成（进度经 Tauri 事件推送）、文档清单

use std::sync::Arc;

use byte_wiki_core::generator::{GenerateEvent, GenerateSummary};
use byte_wiki_core::AppService;
use tauri::Emitter;

use crate::state::AppState;

/// 生成进度事件通道名（前端 listen）
pub const GENERATE_EVENT: &str = "wiki://generate";

/// 一键生成 Wiki（索引 → 规划 → 逐模块 → 组装），进度实时 emit
#[tauri::command]
pub async fn generate_wiki(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<GenerateSummary, String> {
    let svc: Arc<AppService> = Arc::clone(&state.svc);
    let app_handle = app.clone();
    svc.generate_wiki(&path, &move |e: GenerateEvent| {
        let _ = app_handle.emit(GENERATE_EVENT, &e);
    })
    .await
    .map_err(|e| e.to_string())
}

/// 增量更新（FR-06：仅重生成受影响模块 + 反向依赖），进度实时 emit
#[tauri::command]
pub async fn update_wiki(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<byte_wiki_core::generator::UpdateSummary, String> {
    let svc: Arc<AppService> = Arc::clone(&state.svc);
    let app_handle = app.clone();
    svc.update_wiki(&path, &move |e: GenerateEvent| {
        let _ = app_handle.emit(GENERATE_EVENT, &e);
    })
    .await
    .map_err(|e| e.to_string())
}

/// wiki 文档清单 (path, kind, title)
///
/// 克隆加载（FR-08 / F12）：若 db 无登记（他人机器生成的 .bytewiki/），
/// 扫描仓库内 .bytewiki/ 目录登记后返回——只读文件系统，不触发任何 LLM 调用。
#[tauri::command]
pub fn list_wiki_docs(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<Vec<(String, String, String)>, String> {
    let svc = state.svc();
    let repo_root = std::path::Path::new(&path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let Some(repo) = svc
        .db
        .get_repo_by_path(&repo_root.to_string_lossy())
        .map_err(|e| e.to_string())?
    else {
        return Ok(vec![]);
    };

    let mut docs = svc.db.list_wiki_docs(repo.id).map_err(|e| e.to_string())?;
    if docs.is_empty() {
        docs = svc
            .scan_existing_wiki(&repo_root)
            .map_err(|e| e.to_string())?;
    }
    docs.sort();
    Ok(docs)
}

/// 读取 .bytewiki/ 下指定文档内容（路径穿越防护：canonicalize 后必须在 wiki 目录内）
#[tauri::command]
pub fn read_wiki_doc(repo_path: String, doc_path: String) -> Result<String, String> {
    let root = std::path::Path::new(&repo_path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let wiki = root.join(".bytewiki");
    let full = wiki
        .join(&doc_path)
        .canonicalize()
        .map_err(|_| format!("文档不存在：{doc_path}"))?;
    if !full.starts_with(&wiki) {
        return Err(format!("非法路径：{doc_path}"));
    }
    std::fs::read_to_string(&full).map_err(|e| e.to_string())
}

/// 问答流式增量事件通道（前端 listen 拼接）
pub const CHAT_DELTA_EVENT: &str = "wiki://chat-delta";

/// 仓库 RAG 问答：流式增量经事件推送，返回最终答案与出处
#[tauri::command]
pub async fn repo_chat(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
    question: String,
    history: Vec<(String, String)>,
) -> Result<byte_wiki_core::rag::ChatAnswer, String> {
    let svc = Arc::clone(&state.svc);
    let app_handle = app.clone();
    svc.chat_with_repo(&path, &question, &history, &move |delta: &str| {
        let _ = app_handle.emit(CHAT_DELTA_EVENT, delta);
    })
    .await
    .map_err(|e| e.to_string())
}

/// 重建向量索引（克隆加载场景 / embedding 模型更换后）
#[tauri::command]
pub async fn rebuild_embeddings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<byte_wiki_core::rag::EmbedSummary, String> {
    let svc = Arc::clone(&state.svc);
    let app_handle = app.clone();
    svc.rebuild_embeddings(&path, &move |msg: &str| {
        let _ = app_handle.emit(GENERATE_EVENT, &GenerateEvent {
            phase: "embedding".into(),
            current: 0,
            total: 0,
            message: msg.to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
        });
    })
    .await
    .map_err(|e| e.to_string())
}
