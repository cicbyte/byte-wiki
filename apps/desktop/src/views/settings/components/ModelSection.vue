<script setup lang="ts">
/**
 * 模型分区（设置 → 模型）
 *
 * 配置 LLM Provider（FR-09）：本地 Ollama / 远程 OpenAI 兼容端点切换、
 * base_url / 对话模型 / embedding 模型 / API Key，附「测试连接」。
 * 状态在本组件内部创建（沿用既有子组件约定）。
 */
import { ref, onMounted } from "vue";
import { message } from "ant-design-vue";
import { api } from "@/api";
import type { LlmConfig, LlmTestResult } from "@/api/llm";

const PROVIDERS = [
  { value: "ollama", label: "Ollama（本地，代码不出机）" },
  { value: "openai", label: "OpenAI 兼容（远程 API）" },
];

const cfg = ref<LlmConfig>({
  provider: "ollama",
  base_url: "http://localhost:11434",
  chat_model: "",
  embed_model: "",
  api_key: "",
});
const testing = ref(false);
const testResult = ref<LlmTestResult | null>(null);

onMounted(async () => {
  try {
    cfg.value = await api.getLlmConfig();
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "读取模型配置失败");
  }
});

/** 切换 provider 时带出默认 base_url */
function onProviderChange(val: string) {
  cfg.value.provider = val;
  if (val === "ollama" && !cfg.value.base_url) {
    cfg.value.base_url = "http://localhost:11434";
  }
}

async function save() {
  try {
    await api.setLlmConfig(cfg.value);
    message.success("模型配置已保存");
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "保存失败");
  }
}

async function testConnection() {
  testing.value = true;
  testResult.value = null;
  try {
    // 先保存当前表单，再按已保存配置测试
    await api.setLlmConfig(cfg.value);
    testResult.value = await api.testLlmConnection();
  } catch (e: any) {
    testResult.value = {
      ok: false,
      models: [],
      error: typeof e === "string" ? e : "测试失败",
    };
  } finally {
    testing.value = false;
  }
}
</script>

<template>
  <section class="pane">
    <h2 class="pane-title">模型</h2>

    <!-- Provider 类型 -->
    <div class="storage-section">
      <div class="section-head">Provider</div>
      <a-select
        :value="cfg.provider"
        style="width: 100%"
        :options="PROVIDERS"
        @change="onProviderChange"
      />
      <p class="hint">
        本地 Ollama 模式代码不出本机（local-first 默认）；远程模式会把仓库片段发送至第三方服务，请注意隐私。
      </p>
    </div>

    <!-- 服务地址 -->
    <div class="storage-section">
      <div class="section-head">服务地址</div>
      <a-input v-model:value="cfg.base_url" placeholder="http://localhost:11434" />
      <p class="hint">
        Ollama 默认 <code>http://localhost:11434</code>；OpenAI 兼容端点填到 <code>/v1</code> 或宿主根路径均可。
      </p>
    </div>

    <!-- 模型 -->
    <div class="storage-section">
      <div class="section-head">对话 / 生成模型</div>
      <a-input v-model:value="cfg.chat_model" placeholder="如 qwen3-coder:30b" />
      <div class="section-head" style="margin-top: 14px">Embedding 模型</div>
      <a-input v-model:value="cfg.embed_model" placeholder="如 nomic-embed-text" />
      <p class="hint">生成 Wiki 文档与 RAG 问答使用对话模型；向量化检索使用 embedding 模型。</p>
    </div>

    <!-- API Key（仅远程） -->
    <div v-if="cfg.provider === 'openai'" class="storage-section">
      <div class="section-head">API Key</div>
      <a-input-password v-model:value="cfg.api_key" placeholder="sk-..." autocomplete="off" />
      <p class="hint">仅远程模式需要；明文存储于本机 config.toml，随备份导出时请注意。</p>
    </div>

    <!-- 操作 + 测试结果 -->
    <div class="storage-section">
      <div class="backup-actions">
        <button class="storage-btn primary" @click="save">保存</button>
        <button class="storage-btn" :disabled="testing" @click="testConnection">
          {{ testing ? "测试中…" : "测试连接" }}
        </button>
      </div>
      <div v-if="testResult" class="test-result" :class="testResult.ok ? 'ok' : 'fail'">
        <template v-if="testResult.ok">
          连接成功，可用模型 {{ testResult.models.length }} 个：
          <span class="model-list">{{ testResult.models.join("、") || "（未列出）" }}</span>
        </template>
        <template v-else>连接失败：{{ testResult.error }}</template>
      </div>
    </div>
  </section>
</template>

<style scoped>
.pane-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: var(--spacing-md);
}
.storage-section {
  padding: var(--spacing-md) 0;
  border-bottom: 1px solid var(--border-light);
}
.storage-section:last-child {
  border-bottom: none;
}
.section-head {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 10px;
}
.backup-actions {
  display: flex;
  gap: var(--spacing-sm);
}
.storage-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 14px;
  border: 1px solid var(--border-base);
  background: var(--bg-surface);
  color: var(--text-secondary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: 13px;
  transition: all var(--transition-fast);
}
.storage-btn:hover:not(:disabled) {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}
.storage-btn.primary {
  border-color: var(--color-primary);
  background: var(--color-primary);
  color: #fff;
}
.storage-btn.primary:hover:not(:disabled) {
  background: var(--color-primary-hover);
  border-color: var(--color-primary-hover);
  color: #fff;
}
.storage-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.hint {
  color: var(--text-tertiary);
  font-size: 12px;
  margin-top: 8px;
  line-height: 1.6;
}
.hint code {
  background: var(--bg-input);
  padding: 1px 5px;
  border-radius: 3px;
  font-family: "JetBrains Mono", monospace;
  font-size: 11px;
}
.test-result {
  margin-top: 12px;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  font-size: 12px;
  line-height: 1.6;
}
.test-result.ok {
  background: color-mix(in oklch, #16a34a 10%, transparent);
  color: #16a34a;
}
.test-result.fail {
  background: color-mix(in oklch, var(--color-danger) 10%, transparent);
  color: var(--color-danger);
}
.model-list {
  word-break: break-all;
}
</style>
