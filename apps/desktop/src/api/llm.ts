/**
 * 模型 Provider（LLM）command 包装
 */
import { invoke } from "@tauri-apps/api/core";

/** 模型 Provider 配置（与 Rust `LlmConfig` 对应） */
export interface LlmConfig {
  /** ollama / openai（OpenAI 兼容远程端点） */
  provider: string;
  /** 服务基址 */
  base_url: string;
  /** 对话/生成模型 */
  chat_model: string;
  /** embedding 模型 */
  embed_model: string;
  /** 远程模式 API Key（ollama 留空） */
  api_key: string;
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

  testLlmConnection: () => invoke<LlmTestResult>("test_llm_connection"),
};
