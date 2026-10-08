/**
 * 模型供应商 command 包装（多供应商，参考 byte-voice 设置布局）
 */
import { invoke } from "@tauri-apps/api/core";

/** 供应商配置（与 Rust `LlmProvider` 对应） */
export interface LlmProvider {
  id: string;
  name: string;
  /** ollama / openai（OpenAI 兼容端点） */
  kind: string;
  api_endpoint: string;
  api_key: string;
  enabled: boolean;
  models: string[];
  chat_model: string;
  embed_model: string;
}

/** 模型配置（与 Rust `LlmConfig` 对应） */
export interface LlmConfig {
  providers: LlmProvider[];
  active_id: string;
}

/** 测试连接结果 */
export interface LlmTestResult {
  ok: boolean;
  models: string[];
  error: string;
}

export const llmApi = {
  getLlmConfig: () => invoke<LlmConfig>("get_llm_config"),

  setLlmConfig: (config: LlmConfig) => invoke<void>("set_llm_config", { config }),

  testLlmConnection: (providerId: string) =>
    invoke<LlmTestResult>("test_llm_connection", { providerId }),
};
