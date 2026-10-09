/**
 * REST API 服务启停/状态 composable（api-service → 服务管理 tab）
 *
 * 持有 host/port/useAuth 表单状态与运行态（URL/Key），
 * 启停走 api.apiServer*，Key 保存走 api.setApiKeys(restKey)，
 * 文档页经 openDocsInBrowser 唤起系统浏览器（Swagger UI）。
 */
import { ref } from "vue";
import { message } from "ant-design-vue";
import { api } from "@/api";

export function useApiServer() {
  const apiRunning = ref(false);
  const apiUrl = ref("");
  const apiPort = ref(18031);
  const apiHost = ref("127.0.0.1");
  const apiUseAuth = ref(false);
  const apiKey = ref("");
  const apiKeyInput = ref("");
  const apiKeyVisible = ref(false);
  const apiKeySaving = ref(false);
  const apiToggling = ref(false);
  const docsLoading = ref(false);

  /** 查询运行状态 + 回填 Key */
  async function checkStatus() {
    try {
      const s = await api.apiServerStatus();
      apiRunning.value = s.running;
      if (s.running) {
        apiUrl.value = s.url;
        apiPort.value = s.port;
      }
      if (s.api_key) apiKey.value = s.api_key;
    } catch {
      apiRunning.value = false;
    }
  }

  /** 启动 / 停止 */
  async function toggleServer() {
    if (apiToggling.value) return;
    apiToggling.value = true;
    try {
      if (apiRunning.value) {
        await api.apiServerStop();
        apiRunning.value = false;
        apiUrl.value = "";
        message.success("REST API 服务已停止");
      } else {
        const s = await api.apiServerStart(apiPort.value, apiHost.value);
        apiUrl.value = s.url;
        apiRunning.value = s.running;
        if (s.api_key) apiKey.value = s.api_key;
        message.success(`REST API 服务已启动：${s.url}`);
      }
    } catch (e: any) {
      message.error(typeof e === "string" ? e : "操作失败");
    } finally {
      apiToggling.value = false;
    }
  }

  async function copyText(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      message.success("已复制");
    } catch {
      message.error("复制失败");
    }
  }

  /** 保存 REST API Key（写入 config，不影响 MCP Key） */
  async function saveApiKey() {
    const k = apiKeyInput.value.trim();
    if (!k) return;
    apiKeySaving.value = true;
    try {
      await api.setApiKeys(k, null);
      apiKey.value = k;
      apiKeyInput.value = "";
      message.success("API Key 已保存");
    } catch (e: any) {
      message.error(typeof e === "string" ? e : "保存失败");
    } finally {
      apiKeySaving.value = false;
    }
  }

  /** 系统浏览器打开 Swagger 文档 */
  function openDocsInBrowser() {
    const base = apiRunning.value && apiUrl.value ? apiUrl.value : `http://localhost:${apiPort.value}`;
    api.openUrl(`${base}/api/v1/docs`);
  }

  return {
    apiRunning,
    apiUrl,
    apiPort,
    apiHost,
    apiUseAuth,
    apiKey,
    apiKeyInput,
    apiKeyVisible,
    apiKeySaving,
    apiToggling,
    docsLoading,
    checkStatus,
    toggleServer,
    copyText,
    saveApiKey,
    openDocsInBrowser,
  };
}
