<script setup lang="ts">
/**
 * 模型分区（设置 → 模型）— 参考-byte-voice ModelSettings 两栏布局：
 * 左栏「供应商」列表（卡片 + 启用开关 + 使用中标记），右栏供应商详情
 * （API 端点 / 密钥 + 行内检测 / 对话与 Embedding 模型 / 分组模型列表 +
 * 获取模型弹窗 / 设为当前使用）。600ms 节流自动保存。
 * 样式遵循 byte-wiki 现有主题 token。
 */
import { computed, nextTick, onMounted, reactive, ref, watch } from "vue";
import { message } from "ant-design-vue";
import {
  CheckCircleFilled,
  CloseCircleOutlined,
  PlusOutlined,
  DeleteOutlined,
  SyncOutlined,
  RobotOutlined,
} from "@ant-design/icons-vue";
import { api } from "@/api";
import type { LlmConfig, LlmProvider } from "@/api/llm";

const config = ref<LlmConfig>({ providers: [], active_id: "" });
const selectedId = ref("");
const checking = ref(false);
const connectResult = ref<{ ok: boolean; message: string } | null>(null);

// 获取模型弹窗
const fetchModalVisible = ref(false);
const fetching = ref(false);
const fetchedModels = ref<string[]>([]);
const fetchSearchText = ref("");
const selectedFetchModels = ref<string[]>([]);
// 添加模型弹窗
const addModelVisible = ref(false);
const addModelText = ref("");
// 新增供应商弹窗
const addProviderVisible = ref(false);
const newProviderName = ref("");
const newProviderKind = ref("openai");

const KIND_OPTIONS = [
  { value: "ollama", label: "Ollama（本地，代码不出机）" },
  { value: "openai", label: "OpenAI 兼容（远程 API）" },
];

const providers = computed(() => config.value.providers);
const currentProvider = computed(() =>
  providers.value.find((p) => p.id === selectedId.value),
);
const isActive = computed(
  () => selectedId.value !== "" && selectedId.value === config.value.active_id,
);

// 编辑表单（与 byte-voice syncEditForm 模式一致：切换供应商时同步，避免触发自动保存）
const editForm = reactive({
  name: "",
  api_endpoint: "",
  api_key: "",
  kind: "openai",
  enabled: true,
  models: [] as string[],
  chat_model: "",
  embed_model: "",
});
let syncing = false;
let saveTimer: ReturnType<typeof setTimeout> | null = null;

function syncEditForm() {
  const p = currentProvider.value;
  if (!p) return;
  syncing = true;
  editForm.name = p.name;
  editForm.api_endpoint = p.api_endpoint;
  editForm.api_key = p.api_key;
  editForm.kind = p.kind;
  editForm.enabled = p.enabled;
  editForm.models = [...p.models];
  editForm.chat_model = p.chat_model;
  editForm.embed_model = p.embed_model;
  nextTick(() => {
    syncing = false;
  });
}

function selectProvider(id: string) {
  selectedId.value = id;
  connectResult.value = null;
  syncEditForm();
}

async function loadConfig() {
  try {
    config.value = await api.getLlmConfig();
    if (config.value.providers.length && !currentProvider.value) {
      selectProvider(config.value.providers[0].id);
    }
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "读取模型配置失败");
  }
}
onMounted(loadConfig);

// ============ 自动保存（600ms 节流，参考 byte-voice） ============

function scheduleSave() {
  if (syncing) return;
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(doAutoSave, 600);
}

async function doAutoSave() {
  const p = currentProvider.value;
  if (!p) return;
  p.name = editForm.name;
  p.api_endpoint = editForm.api_endpoint;
  p.api_key = editForm.api_key;
  p.kind = editForm.kind;
  p.enabled = editForm.enabled;
  p.models = [...editForm.models];
  p.chat_model = editForm.chat_model;
  p.embed_model = editForm.embed_model;
  try {
    await api.setLlmConfig(config.value);
  } catch (e: any) {
    console.error("自动保存失败", e);
  }
}

watch(
  () => ({ ...editForm }),
  () => scheduleSave(),
  { deep: true },
);

// ============ 供应商增删 / 设为当前使用 ============

function showAddProvider() {
  newProviderName.value = "";
  newProviderKind.value = "openai";
  addProviderVisible.value = true;
}

function confirmAddProvider() {
  const name = newProviderName.value.trim();
  if (!name) return;
  const id = `p-${Date.now().toString(36)}`;
  config.value.providers.push({
    id,
    name,
    kind: newProviderKind.value,
    api_endpoint: newProviderKind.value === "ollama" ? "http://localhost:11434" : "",
    api_key: "",
    enabled: true,
    models: [],
    chat_model: "",
    embed_model: "",
  });
  addProviderVisible.value = false;
  api.setLlmConfig(config.value).then(() => message.success(`已添加供应商「${name}」`));
  selectProvider(id);
}

async function removeProvider() {
  const p = currentProvider.value;
  if (!p) return;
  if (p.id === config.value.active_id) {
    message.warning("当前使用中的供应商不能删除，请先切换");
    return;
  }
  config.value.providers = config.value.providers.filter((x) => x.id !== p.id);
  if (selectedId.value === p.id) {
    selectProvider(config.value.providers[0]?.id ?? "");
  }
  await api.setLlmConfig(config.value);
  message.success(`已删除「${p.name}」`);
}

async function setActive() {
  const p = currentProvider.value;
  if (!p) return;
  if (!p.enabled) {
    message.warning("供应商已禁用，请先启用");
    return;
  }
  config.value.active_id = p.id;
  await api.setLlmConfig(config.value);
  message.success(`「${p.name}」已设为当前使用`);
}

async function toggleProvider(p: LlmProvider, val: boolean) {
  p.enabled = val;
  // 禁用当前使用的供应商时提示
  if (!val && p.id === config.value.active_id) {
    message.warning("已禁用当前使用的供应商，生成/问答将不可用");
  }
  await api.setLlmConfig(config.value);
}

// ============ 连通性检测（先静默保存再检测） ============

async function handleCheckConnectivity() {
  if (!selectedId.value) return;
  checking.value = true;
  connectResult.value = null;
  try {
    await doAutoSave();
    const r = await api.testLlmConnection(selectedId.value);
    connectResult.value = r.ok
      ? { ok: true, message: `连接成功，发现 ${r.models.length} 个模型` }
      : { ok: false, message: r.error };
    // 顺带把远端模型合并进待选（不覆盖已选）
    if (r.ok && r.models.length) {
      for (const m of r.models) {
        if (!editForm.models.includes(m)) editForm.models.push(m);
      }
    }
  } catch (e: any) {
    connectResult.value = { ok: false, message: typeof e === "string" ? e : "检测失败" };
  } finally {
    checking.value = false;
  }
}

// ============ 模型列表（分组 / 获取 / 添加 / 删除） ============

/** 按模型 id 前缀分组（qwen3.5:9b → Qwen） */
function modelGroup(id: string): string {
  const prefix = (id.split(/[-_./:]/)[0] ?? "").toLowerCase();
  if (!prefix) return "其他";
  const map: Record<string, string> = {
    gpt: "GPT", o1: "OpenAI o-series", o3: "OpenAI o-series", claude: "Claude",
    deepseek: "DeepSeek", qwen: "Qwen", glm: "GLM", llama: "LLaMA", gemma: "Gemma",
    mistral: "Mistral", moonshot: "Moonshot", nomic: "Embedding", bge: "Embedding",
    mxbai: "Embedding", snowflake: "Embedding",
  };
  for (const [k, v] of Object.entries(map)) {
    if (prefix.includes(k)) return v;
  }
  return prefix.charAt(0).toUpperCase() + prefix.slice(1);
}

const modelGroups = computed(() => {
  const groups: Record<string, string[]> = {};
  for (const m of editForm.models) {
    const g = modelGroup(m);
    (groups[g] ??= []).push(m);
  }
  return groups;
});

const fetchedModelGroups = computed(() => {
  const q = fetchSearchText.value.toLowerCase();
  const list = q ? fetchedModels.value.filter((m) => m.toLowerCase().includes(q)) : fetchedModels.value;
  const groups: Record<string, string[]> = {};
  for (const m of list) {
    const g = modelGroup(m);
    (groups[g] ??= []).push(m);
  }
  return groups;
});

async function showFetchModels() {
  fetchModalVisible.value = true;
  fetching.value = true;
  fetchedModels.value = [];
  selectedFetchModels.value = [];
  fetchSearchText.value = "";
  try {
    await doAutoSave();
    const r = await api.testLlmConnection(selectedId.value);
    if (!r.ok) {
      message.error("获取模型失败：" + r.error);
      fetchModalVisible.value = false;
      return;
    }
    fetchedModels.value = r.models;
    selectedFetchModels.value = editForm.models.filter((m) => r.models.includes(m));
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "获取模型失败");
    fetchModalVisible.value = false;
  } finally {
    fetching.value = false;
  }
}

function toggleFetchModel(m: string) {
  const i = selectedFetchModels.value.indexOf(m);
  if (i >= 0) selectedFetchModels.value.splice(i, 1);
  else selectedFetchModels.value.push(m);
}

function toggleGroupAll(models: string[]) {
  const all = models.every((m) => selectedFetchModels.value.includes(m));
  if (all) {
    selectedFetchModels.value = selectedFetchModels.value.filter((m) => !models.includes(m));
  } else {
    for (const m of models) {
      if (!selectedFetchModels.value.includes(m)) selectedFetchModels.value.push(m);
    }
  }
}

function confirmFetchModels() {
  editForm.models = [...selectedFetchModels.value];
  if (editForm.models.length && !editForm.models.includes(editForm.chat_model)) {
    editForm.chat_model = editForm.models[0];
  }
  fetchModalVisible.value = false;
  message.success(`已选择 ${editForm.models.length} 个模型`);
}

function confirmAddModel() {
  const names = addModelText.value.split(/[,，\n]/).map((s) => s.trim()).filter(Boolean);
  let added = 0;
  for (const n of names) {
    if (!editForm.models.includes(n)) {
      editForm.models.push(n);
      added++;
    }
  }
  addModelVisible.value = false;
  if (added > 0) message.success(`已添加 ${added} 个模型`);
  else message.warning("没有新模型可添加");
}

function removeModel(name: string) {
  editForm.models = editForm.models.filter((m) => m !== name);
  if (editForm.chat_model === name) editForm.chat_model = editForm.models[0] ?? "";
  if (editForm.embed_model === name) editForm.embed_model = editForm.models[0] ?? "";
}
</script>

<template>
  <section class="model-pane">
    <div class="model-container">
      <!-- 左栏：供应商列表 -->
      <div class="provider-list-panel">
        <div class="panel-header">
          <span class="panel-title">供应商</span>
          <span class="provider-count">{{ providers.length }}</span>
        </div>
        <div class="provider-list">
          <div
            v-for="p in providers"
            :key="p.id"
            class="provider-card"
            :class="{ active: selectedId === p.id }"
            @click="selectProvider(p.id)"
          >
            <div class="provider-info">
              <div class="provider-name">
                {{ p.name }}
                <span v-if="p.id === config.active_id" class="active-tag">使用中</span>
              </div>
              <div class="provider-meta">{{ p.models.length }} 个模型</div>
            </div>
            <div class="provider-toggle" @click.stop>
              <a-switch :checked="p.enabled" size="small" @change="(val: any) => toggleProvider(p, val)" />
            </div>
          </div>
        </div>
        <button class="add-provider-btn" @click="showAddProvider">
          <PlusOutlined /> 添加供应商
        </button>
      </div>

      <!-- 右栏：供应商详情 -->
      <div v-if="currentProvider" class="provider-detail-panel">
        <div class="detail-header">
          <div class="detail-name">{{ editForm.name }}</div>
          <div class="detail-description">
            {{ editForm.kind === "ollama" ? "本地 Ollama 服务，代码不出本机" : "OpenAI 兼容远程端点，代码片段将发送至第三方" }}
          </div>
        </div>

        <!-- 名称 / 类型 -->
        <div class="setting-group">
          <div class="setting-title">名称</div>
          <a-input v-model:value="editForm.name" size="small" style="width: 100%" />
        </div>
        <div class="setting-group">
          <div class="setting-title">协议类型</div>
          <a-select v-model:value="editForm.kind" size="small" style="width: 100%" :options="KIND_OPTIONS" />
        </div>

        <!-- API 端点 -->
        <div class="setting-group">
          <div class="setting-title">API 端点</div>
          <a-input
            v-model:value="editForm.api_endpoint"
            placeholder="http://localhost:11434"
            size="small"
            style="width: 100%"
          />
        </div>

        <!-- API 密钥 -->
        <div class="setting-group">
          <div class="setting-title-row">
            <span>API 密钥</span>
            <a-tag v-if="editForm.api_key" color="green" style="font-size: 11px; line-height: 1">已设置</a-tag>
            <a-tag v-else color="red" style="font-size: 11px; line-height: 1">未设置</a-tag>
          </div>
          <div class="api-key-row">
            <a-input-password
              v-model:value="editForm.api_key"
              :placeholder="editForm.kind === 'ollama' ? '本地服务通常无需密钥' : 'sk-...'"
              size="small"
              style="flex: 1"
            />
            <button class="storage-btn check-btn" :class="{ ok: connectResult?.ok }" :disabled="checking" @click="handleCheckConnectivity">
              <CheckCircleFilled v-if="connectResult?.ok" style="color: #52c41a" />
              <CloseCircleOutlined v-else-if="connectResult && !connectResult.ok" style="color: #ff4d4f" />
              {{ checking ? "检测中…" : "检测" }}
            </button>
          </div>
          <div v-if="connectResult" class="connect-result" :class="{ success: connectResult.ok, error: !connectResult.ok }">
            <CheckCircleFilled v-if="connectResult.ok" />
            <CloseCircleOutlined v-else />
            <span>{{ connectResult.message }}</span>
          </div>
        </div>

        <!-- 默认模型 -->
        <div class="setting-group">
          <div class="setting-title">对话 / 生成模型</div>
          <a-select
            v-model:value="editForm.chat_model"
            size="small"
            style="width: 100%"
            :options="editForm.models.map((m) => ({ value: m, label: m }))"
            placeholder="选择或获取模型"
            allow-clear
          />
        </div>
        <div class="setting-group">
          <div class="setting-title">Embedding 模型</div>
          <a-select
            v-model:value="editForm.embed_model"
            size="small"
            style="width: 100%"
            :options="editForm.models.map((m) => ({ value: m, label: m }))"
            placeholder="RAG 向量化模型"
            allow-clear
          />
        </div>

        <!-- 模型列表 -->
        <div class="setting-group">
          <div class="setting-title-row">
            <span>模型列表</span>
            <a-tag>{{ editForm.models.length }}</a-tag>
            <div style="flex: 1" />
            <button class="storage-btn mini-btn" :disabled="fetching" @click="showFetchModels">
              <SyncOutlined /> {{ fetching ? "获取中…" : "获取模型" }}
            </button>
            <button class="storage-btn mini-btn" style="margin-left: 4px" @click="addModelVisible = true; addModelText = ''">
              <PlusOutlined /> 添加
            </button>
          </div>
          <div v-if="editForm.models.length" class="model-groups">
            <a-collapse :bordered="false" class="model-collapse">
              <a-collapse-panel v-for="(models, group) in modelGroups" :key="group" :header="group">
                <template #extra><a-tag>{{ models.length }}</a-tag></template>
                <div v-for="m in models" :key="m" class="model-item">
                  <span class="model-name" :title="`点击设为对话模型：${m}`" @click="editForm.chat_model = m">{{ m }}</span>
                  <div class="model-actions">
                    <a-tag v-if="m === editForm.chat_model" color="blue" style="font-size: 10px; line-height: 1">对话</a-tag>
                    <a-tag v-if="m === editForm.embed_model" color="purple" style="font-size: 10px; line-height: 1">Embed</a-tag>
                    <a-button type="text" size="small" danger @click="removeModel(m)">
                      <DeleteOutlined />
                    </a-button>
                  </div>
                </div>
              </a-collapse-panel>
            </a-collapse>
          </div>
          <div v-else class="model-empty">暂无模型，点击"获取模型"或"添加"</div>
        </div>

        <!-- 启用 / 当前使用 -->
        <div class="setting-group">
          <div class="setting-row-inline">
            <div>
              <div class="setting-row-label">启用此供应商</div>
              <div class="setting-row-desc">禁用后不可被生成 / 问答使用</div>
            </div>
            <a-switch v-model:checked="editForm.enabled" size="small" />
          </div>
        </div>
        <div class="setting-group detail-actions">
          <button v-if="!isActive" class="storage-btn primary" @click="setActive">
            <RobotOutlined /> 设为当前使用
          </button>
          <span v-else class="in-use-hint">当前生成与问答使用此供应商</span>
          <button class="storage-btn danger-btn" @click="removeProvider">
            <DeleteOutlined /> 删除
          </button>
        </div>
      </div>

      <!-- 空状态 -->
      <div v-else class="empty-state">
        <RobotOutlined style="font-size: 32px; opacity: 0.3" />
        <p>添加并选择一个供应商查看配置</p>
      </div>
    </div>

    <!-- 获取模型弹窗 -->
    <a-modal
      v-model:open="fetchModalVisible"
      title="获取模型列表"
      width="600px"
      ok-text="确认选择"
      cancel-text="取消"
      @ok="confirmFetchModels"
    >
      <a-spin :spinning="fetching">
        <div style="margin-bottom: 12px">
          <a-input-search v-model:value="fetchSearchText" placeholder="搜索模型…" allow-clear size="small" />
        </div>
        <div class="fetch-model-hint">
          已选 {{ selectedFetchModels.length }} / {{ fetchedModels.length }} 个模型
        </div>
        <div v-if="fetchedModels.length" class="fetch-model-list">
          <a-collapse :bordered="false" size="small">
            <a-collapse-panel v-for="(models, group) in fetchedModelGroups" :key="group">
              <template #header>
                <div style="display: flex; align-items: center; gap: 8px" @click.stop="toggleGroupAll(models)">
                  <a-checkbox
                    :checked="models.every((m) => selectedFetchModels.includes(m))"
                    :indeterminate="models.some((m) => selectedFetchModels.includes(m)) && !models.every((m) => selectedFetchModels.includes(m))"
                  />
                  <span>{{ group }}</span>
                  <a-tag>{{ models.length }}</a-tag>
                </div>
              </template>
              <div v-for="m in models" :key="m" class="fetch-model-item" @click="toggleFetchModel(m)">
                <a-checkbox :checked="selectedFetchModels.includes(m)" />
                <span class="fetch-model-name">{{ m }}</span>
              </div>
            </a-collapse-panel>
          </a-collapse>
        </div>
        <div v-else-if="!fetching" class="model-empty">未获取到模型</div>
      </a-spin>
    </a-modal>

    <!-- 添加模型弹窗 -->
    <a-modal v-model:open="addModelVisible" title="添加模型" ok-text="添加" cancel-text="取消" @ok="confirmAddModel">
      <div style="margin-bottom: 8px; color: var(--text-tertiary); font-size: 12px">支持逗号分隔批量添加</div>
      <a-textarea v-model:value="addModelText" placeholder="qwen3.5:9b, nomic-embed-text" :auto-size="{ minRows: 2, maxRows: 5 }" />
    </a-modal>

    <!-- 新增供应商弹窗 -->
    <a-modal v-model:open="addProviderVisible" title="添加供应商" ok-text="添加" cancel-text="取消" @ok="confirmAddProvider">
      <div class="setting-title" style="margin-bottom: 8px">名称</div>
      <a-input v-model:value="newProviderName" placeholder="如 DeepSeek / SiliconFlow / 本地 Ollama" style="margin-bottom: 14px" />
      <div class="setting-title" style="margin-bottom: 8px">协议类型</div>
      <a-select v-model:value="newProviderKind" style="width: 100%" :options="KIND_OPTIONS" />
    </a-modal>
  </section>
</template>

<style scoped>
.model-pane {
  display: flex;
  flex: 1;
  min-height: 0;
  min-width: 0;
}

/* 两栏容器（全出血：撑满设置内容区） */
.model-container {
  display: flex;
  flex: 1;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
  background: var(--bg-card);
}

/* 左栏供应商列表 */
.provider-list-panel {
  width: 220px;
  min-width: 220px;
  border-right: 1px solid var(--border-light);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
}
.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 14px 8px;
  user-select: none;
}
.panel-title {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
.provider-count {
  font-size: 11px;
  color: var(--text-secondary);
  background: var(--bg-input);
  border: 1px solid var(--border-base);
  border-radius: 10px;
  padding: 1px 8px;
  font-family: "JetBrains Mono", monospace;
}
.provider-list {
  flex: 1;
  overflow-y: auto;
  padding: 0 8px 8px;
}
.provider-card {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px;
  cursor: pointer;
  border-radius: var(--radius-md);
  border-left: 3px solid transparent;
  transition: all var(--transition-fast);
  margin-bottom: 2px;
}
.provider-card:hover {
  background: var(--bg-elevated);
}
.provider-card.active {
  background: var(--color-primary-bg);
  border-left-color: var(--color-primary);
}
.provider-info {
  flex: 1;
  min-width: 0;
}
.provider-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  display: flex;
  align-items: center;
  gap: 6px;
}
.active-tag {
  font-size: 10px;
  font-weight: 600;
  color: var(--color-primary);
  background: color-mix(in oklch, var(--color-primary) 14%, transparent);
  border-radius: var(--radius-xs);
  padding: 0 6px;
  flex-shrink: 0;
}
.provider-meta {
  margin-top: 2px;
  font-size: 11px;
  color: var(--text-tertiary);
}
.provider-toggle {
  flex-shrink: 0;
}
.add-provider-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  margin: 0 8px 10px;
  padding: 8px;
  border: 1px dashed var(--border-base);
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all var(--transition-fast);
}
.add-provider-btn:hover {
  color: var(--color-primary);
  border-color: var(--color-primary-border);
}

/* 右栏详情 */
.provider-detail-panel {
  flex: 1;
  padding: 18px 22px;
  overflow-y: auto;
}
.detail-header {
  margin-bottom: 16px;
  padding-bottom: 14px;
  border-bottom: 1px solid var(--border-light);
}
.detail-name {
  font-size: 17px;
  font-weight: 600;
  color: var(--text-primary);
}
.detail-description {
  font-size: 12px;
  color: var(--text-tertiary);
  margin-top: 4px;
  line-height: 1.5;
}
.setting-group {
  margin-bottom: 16px;
}
.setting-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 8px;
}
.setting-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 8px;
}
.api-key-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.check-btn {
  height: 26px;
  padding: 0 10px;
  font-size: 12px;
}
.check-btn.ok {
  border-color: #52c41a;
  color: #52c41a;
}
.connect-result {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
  font-size: 12px;
  padding: 6px 10px;
  border-radius: var(--radius-sm);
}
.connect-result.success {
  color: #52c41a;
  background: rgba(82, 196, 26, 0.08);
}
.connect-result.error {
  color: var(--color-danger);
  background: color-mix(in oklch, var(--color-danger) 8%, transparent);
}
.setting-row-inline {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.setting-row-label {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
}
.setting-row-desc {
  font-size: 12px;
  color: var(--text-tertiary);
  margin-top: 2px;
}
.detail-actions {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
}
.in-use-hint {
  font-size: 12px;
  color: var(--color-primary);
  flex: 1;
}
.danger-btn:hover:not(:disabled) {
  border-color: var(--color-danger);
  color: var(--color-danger);
}

/* 模型列表 */
.model-groups {
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  overflow: hidden;
}
.model-collapse {
  background: transparent;
}
.model-collapse :deep(.ant-collapse-item) {
  border-bottom: 1px solid var(--border-light);
}
.model-collapse :deep(.ant-collapse-item:last-child) {
  border-bottom: none;
}
.model-collapse :deep(.ant-collapse-header) {
  padding: 9px 12px !important;
  align-items: center !important;
}
.model-collapse :deep(.ant-collapse-content-box) {
  padding: 0 12px !important;
}
.model-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 0;
  border-bottom: 1px solid var(--border-light);
}
.model-item:last-child {
  border-bottom: none;
}
.model-name {
  font-size: 12px;
  font-family: "JetBrains Mono", monospace;
  color: var(--text-primary);
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.model-name:hover {
  color: var(--color-primary);
}
.model-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.model-empty {
  padding: 18px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 12px;
  border: 1px dashed var(--border-base);
  border-radius: var(--radius-sm);
}
.mini-btn {
  height: 26px;
  padding: 0 10px;
  font-size: 12px;
}

/* 获取模型弹窗 */
.fetch-model-hint {
  margin-bottom: 8px;
  font-size: 12px;
  color: var(--text-tertiary);
}
.fetch-model-list {
  max-height: 400px;
  overflow-y: auto;
}
.fetch-model-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 4px;
  cursor: pointer;
  border-radius: var(--radius-xs);
}
.fetch-model-item:hover {
  background: var(--bg-elevated);
}
.fetch-model-name {
  font-size: 12px;
  font-family: "JetBrains Mono", monospace;
  color: var(--text-primary);
}

/* 空状态 */
.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
  gap: 12px;
}
.empty-state p {
  font-size: 13px;
  margin: 0;
}

/* 复用设置页按钮规范 */
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
</style>
