<script setup lang="ts">
/**
 * 服务管理面板（mcp-service → 服务管理 tab）
 *
 * 可变状态（host/port/useAuth/keyInput/keyVisible）通过 defineModel 与主入口双向绑定，
 * 状态唯一来源仍在主入口的 useMcpServer；本组件不持有同名状态副本。
 * 启停/保存/复制以事件上报，由主入口调用 composable 函数。
 */
import { CopyOutlined } from "@ant-design/icons-vue";
import AlertNotice from "@/components/common/AlertNotice.vue";

const mcpHost = defineModel<string>("host");
const mcpPort = defineModel<number>("port");
const mcpUseAuth = defineModel<boolean>("useAuth");
const mcpKeyInput = defineModel<string>("keyInput");
const mcpKeyVisible = defineModel<boolean>("keyVisible");

defineProps<{
  mcpRunning: boolean;
  mcpUrl: string;
  mcpKey: string;
  mcpToggling: boolean;
  mcpKeySaving: boolean;
  hostOptions: { value: string; label: string }[];
}>();

const emit = defineEmits<{
  (e: "toggle"): void;
  (e: "save-key"): void;
  (e: "copy-url"): void;
  (e: "copy", text: string): void;
}>();

function toggleMcpServer() {
  emit("toggle");
}
function saveMcpKey() {
  emit("save-key");
}
function copyMcpUrl() {
  emit("copy-url");
}
function copyText(text: string) {
  emit("copy", text);
}
</script>

<template>
  <div class="server-panel">
    <!-- 状态行 -->
    <div class="row">
      <span class="row-label">状态</span>
      <div class="row-value">
        <span class="dot" :class="{ on: mcpRunning }"></span>
        <span :class="mcpRunning ? 'state-on' : 'state-off'">{{ mcpRunning ? "运行中" : "已停止" }}</span>
      </div>
    </div>

    <!-- 监听 + 启停 -->
    <div class="row">
      <span class="row-label">监听</span>
      <div class="row-value">
        <a-select v-model:value="mcpHost" :disabled="mcpRunning" size="small" style="width: 130px" :options="hostOptions" />
        <span class="colon">:</span>
        <input v-model="mcpPort" type="text" inputmode="numeric" class="port-field" :disabled="mcpRunning" />
        <button class="toggle-btn" :class="mcpRunning ? 'stop' : 'start'" :disabled="mcpToggling" @click="toggleMcpServer">
          {{ mcpToggling ? "…" : mcpRunning ? "停止" : "启动" }}
        </button>
      </div>
    </div>

    <!-- 鉴权 -->
    <div class="row">
      <span class="row-label">鉴权</span>
      <div class="row-value">
        <a-switch v-model:checked="mcpUseAuth" size="small" :disabled="mcpRunning" />
        <span class="auth-hint">{{ mcpUseAuth ? "需 MCP Key 认证" : "无认证（仅建议内网）" }}</span>
      </div>
    </div>

    <!-- MCP Key -->
    <div v-if="mcpUseAuth" class="row">
      <span class="row-label">MCP Key</span>
      <div class="row-value">
        <template v-if="!mcpRunning">
          <input
            v-model="mcpKeyInput"
            class="key-field"
            placeholder="留空则用 config 中的 key"
            @blur="saveMcpKey"
          />
          <button class="mini-btn" :disabled="mcpKeySaving" @click="saveMcpKey">保存</button>
        </template>
        <template v-else>
          <code class="copy-field key">{{ mcpKeyVisible ? mcpKey : "•••••••••••••••••••••••••••••" }}</code>
          <button class="mini-btn" @click="mcpKeyVisible = !mcpKeyVisible">{{ mcpKeyVisible ? "隐藏" : "显示" }}</button>
          <button class="mini-btn" title="复制" @click="copyText(mcpKey)"><CopyOutlined /></button>
        </template>
      </div>
    </div>

    <!-- 端点 -->
    <div v-if="mcpRunning && mcpUrl" class="row">
      <span class="row-label">端点</span>
      <div class="row-value">
        <code class="copy-field">{{ mcpUrl }}</code>
        <button class="mini-btn" title="复制" @click="copyMcpUrl"><CopyOutlined /></button>
      </div>
    </div>
  </div>

  <p class="hint">
    供 AI 工具通过 MCP 协议检索知识库。stdio 模式无需在此启动，见「配置」标签。
  </p>
  <AlertNotice v-if="!mcpUseAuth" type="warning" style="margin-top: 8px;">
    当前无鉴权，绑定 <code>0.0.0.0</code> 时任何人都可访问。
  </AlertNotice>
  <AlertNotice v-else type="success" style="margin-top: 8px;">
    鉴权已开启，调用需携带 MCP Key。
  </AlertNotice>
</template>

<style scoped>
/* 服务管理面板（与 REST API 页统一） */
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
.row-label { width: 80px; flex-shrink: 0; font-size: 12px; color: var(--text-tertiary); }
.row-value { display: flex; align-items: center; gap: 8px; flex: 1; min-width: 0; }
.dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-disabled); flex-shrink: 0; }
.dot.on { background: var(--color-success); box-shadow: 0 0 6px var(--color-success); }
.state-on { font-size: 13px; color: var(--color-success); font-weight: 600; }
.state-off { font-size: 13px; color: var(--text-tertiary); }
.colon { color: var(--text-tertiary); font-family: "JetBrains Mono", monospace; font-size: 13px; }
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
.toggle-btn.start { background: var(--color-primary); color: #fff; }
.toggle-btn.start:hover:not(:disabled) { background: var(--color-primary-hover); }
.toggle-btn.stop { background: var(--danger-bg); color: var(--color-danger); border: 1px solid var(--color-danger); }
.toggle-btn.stop:hover:not(:disabled) { background: var(--color-danger); color: #fff; }
.toggle-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.copy-field {
  flex: 1; min-width: 0;
  font-family: "JetBrains Mono", monospace;
  font-size: 12px;
  padding: 5px 10px;
  border-radius: var(--radius-sm);
  background: var(--bg-input);
  color: var(--text-secondary);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.mini-btn {
  height: 28px; padding: 0 10px;
  border: 1px solid var(--border-base);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-secondary);
  font-size: 12px; cursor: pointer;
  transition: all var(--transition-fast);
  flex-shrink: 0;
  display: inline-flex; align-items: center;
}
.mini-btn:hover { border-color: var(--color-primary-border); color: var(--color-primary); }
.auth-hint { font-size: 12px; color: var(--text-tertiary); }
.key-field {
  flex: 1; min-width: 0;
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
.hint { color: var(--text-tertiary); font-size: 12px; line-height: 1.6; }
.hint code { background: var(--bg-input); padding: 1px 5px; border-radius: 3px; font-family: "JetBrains Mono", monospace; font-size: 11px; }
</style>
