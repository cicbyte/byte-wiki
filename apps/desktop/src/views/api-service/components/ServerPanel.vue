<script setup lang="ts">
/**
 * 服务管理面板（api-service → 服务管理 tab）
 *
 * 可变状态（host/port/useAuth/keyInput/keyVisible）通过 defineModel 与主入口双向绑定，
 * 状态唯一来源仍在主入口的 useApiServer；本组件不持有同名状态副本。
 * 启停/保存/复制以事件上报，由主入口调用 composable 函数。
 */
import { CopyOutlined } from "@ant-design/icons-vue";
import AlertNotice from "@/components/common/AlertNotice.vue";

const apiHost = defineModel<string>("host");
const apiPort = defineModel<number>("port");
const apiUseAuth = defineModel<boolean>("useAuth");
const apiKeyInput = defineModel<string>("keyInput");
const apiKeyVisible = defineModel<boolean>("keyVisible");

defineProps<{
  apiRunning: boolean;
  apiUrl: string;
  apiKey: string;
  apiToggling: boolean;
  apiKeySaving: boolean;
  hostOptions: { value: string; label: string }[];
}>();

const emit = defineEmits<{
  (e: "toggle"): void;
  (e: "save-key"): void;
  (e: "copy", text: string): void;
}>();

function toggleServer() {
  emit("toggle");
}
function saveApiKey() {
  emit("save-key");
}
function copyText(text: string) {
  emit("copy", text);
}
</script>

<template>
  <div class="server-panel">
    <!-- 状态 -->
    <div class="row">
      <span class="row-label">状态</span>
      <div class="row-value">
        <span class="dot" :class="{ on: apiRunning }"></span>
        <span :class="apiRunning ? 'state-on' : 'state-off'">{{ apiRunning ? "运行中" : "已停止" }}</span>
      </div>
    </div>

    <!-- 监听 + 启停 -->
    <div class="row">
      <span class="row-label">监听</span>
      <div class="row-value">
        <a-select v-model:value="apiHost" :disabled="apiRunning" size="small" style="width: 130px" :options="hostOptions" />
        <span class="colon">:</span>
        <input v-model="apiPort" type="text" inputmode="numeric" class="port-field" :disabled="apiRunning" />
        <button class="toggle-btn" :class="apiRunning ? 'stop' : 'start'" :disabled="apiToggling" @click="toggleServer">
          {{ apiToggling ? "…" : apiRunning ? "停止" : "启动" }}
        </button>
      </div>
    </div>

    <!-- 鉴权 -->
    <div class="row">
      <span class="row-label">鉴权</span>
      <div class="row-value">
        <a-switch v-model:checked="apiUseAuth" size="small" :disabled="apiRunning" />
        <span class="auth-hint">{{ apiUseAuth ? "需 API Key 认证" : "无认证（仅建议内网）" }}</span>
      </div>
    </div>

    <!-- API Key（鉴权开启时） -->
    <div v-if="apiUseAuth" class="row">
      <span class="row-label">API Key</span>
      <div class="row-value">
        <template v-if="!apiRunning">
          <input v-model="apiKeyInput" class="key-field" :placeholder="apiKey || '输入自定义 key'" @blur="saveApiKey" />
          <button class="mini-btn" :disabled="apiKeySaving" @click="saveApiKey">保存</button>
        </template>
        <template v-else>
          <code class="copy-field key">{{ apiKeyVisible ? apiKey : "•••••••••••••••••••••••••••••" }}</code>
          <button class="mini-btn" @click="apiKeyVisible = !apiKeyVisible">{{ apiKeyVisible ? "隐藏" : "显示" }}</button>
          <button class="mini-btn" title="复制" @click="copyText(apiKey)"><CopyOutlined /></button>
        </template>
      </div>
    </div>

    <!-- 端点 -->
    <div v-if="apiRunning && apiUrl" class="row">
      <span class="row-label">端点</span>
      <div class="row-value">
        <code class="copy-field">{{ apiUrl }}</code>
        <button class="mini-btn" title="复制" @click="copyText(apiUrl)"><CopyOutlined /></button>
      </div>
    </div>
  </div>

  <p class="hint">
    供第三方应用通过标准 HTTP 对接知识库。CLI：<code>bs api serve</code>。端点文档见「API 文档」。
  </p>
  <AlertNotice v-if="!apiUseAuth" type="warning" style="margin-top: 8px;">
    当前无鉴权，绑定 <code>0.0.0.0</code> 时任何人都可访问。
  </AlertNotice>
</template>

<style scoped>
/* 服务管理面板 */
.server-panel {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-md);
  overflow: hidden;
}
.row {
  display: flex;
  align-items: center;
  min-height: 46px;
  padding: 0 16px;
  border-bottom: 1px solid var(--border-light);
}
.row:last-child { border-bottom: none; }
.row-label {
  width: 80px;
  flex-shrink: 0;
  font-size: 12px;
  color: var(--text-tertiary);
}
.row-value {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 0;
}
/* 状态 */
.dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-disabled); flex-shrink: 0; }
.dot.on { background: var(--color-success); box-shadow: 0 0 6px var(--color-success); }
.state-on { font-size: 13px; color: var(--color-success); font-weight: 600; }
.state-off { font-size: 13px; color: var(--text-tertiary); }
/* 端口 */
.port-field {
  width: 90px;
  padding: 5px 10px;
  border: 1px solid var(--border-base);
  border-radius: var(--radius-sm);
  background: var(--bg-input);
  color: var(--text-primary);
  font-family: "JetBrains Mono", monospace;
  font-size: 13px;
  outline: none;
  flex-shrink: 0;
}
.port-field:focus { border-color: var(--color-primary); }
.port-field:disabled { opacity: 0.5; }
.colon { color: var(--text-tertiary); font-family: "JetBrains Mono", monospace; font-size: 13px; }
.key-field {
  flex: 1;
  min-width: 0;
  padding: 5px 10px;
  border: 1px solid var(--border-base);
  border-radius: var(--radius-sm);
  background: var(--bg-input);
  color: var(--text-primary);
  font-family: "JetBrains Mono", monospace;
  font-size: 12px;
  outline: none;
}
.key-field:focus { border-color: var(--color-primary); }
.key-field::placeholder { color: var(--text-disabled); }
/* 启停按钮 */
.toggle-btn {
  height: 30px;
  padding: 0 20px;
  border: none;
  border-radius: var(--radius-sm);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all var(--transition-fast);
  flex-shrink: 0;
}
/* 主钮底/墨走主题互斥令牌（明暗各配）——写死 #fff 在暗色浅主色下白上白看不清 */
.toggle-btn.start {
  background: var(--btn-primary-bg);
  color: var(--btn-primary-ink);
}
.toggle-btn.start:hover:not(:disabled) { background: color-mix(in srgb, var(--btn-primary-ink) 12%, var(--btn-primary-bg)); }
.toggle-btn.stop {
  background: var(--danger-bg);
  color: var(--color-danger);
  border: 1px solid var(--color-danger);
}
.toggle-btn.stop:hover:not(:disabled) { background: var(--color-danger); color: #fff; }
.toggle-btn:disabled { opacity: 0.5; cursor: not-allowed; }
/* 可复制字段 */
.copy-field {
  flex: 1;
  min-width: 0;
  font-family: "JetBrains Mono", monospace;
  font-size: 12px;
  padding: 5px 10px;
  border-radius: var(--radius-sm);
  background: var(--bg-input);
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.copy-field.key { letter-spacing: 1px; }
/* 迷你按钮 */
.mini-btn {
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--border-base);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all var(--transition-fast);
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
}
.mini-btn:hover { border-color: var(--color-primary-border); color: var(--color-primary); }

/* 提示文字 */
.hint { color: var(--text-tertiary); font-size: 12px; line-height: 1.6; }
.hint code { background: var(--bg-input); padding: 1px 5px; border-radius: 3px; font-family: "JetBrains Mono", monospace; font-size: 11px; }
.auth-hint { font-size: 12px; color: var(--text-tertiary); }
</style>
