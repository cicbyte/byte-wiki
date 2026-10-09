/**
 * MCP 服务启停/状态 composable（mcp-service → 服务管理 tab）
 *
 * 持有 host/port/useAuth 表单状态与运行态（URL/Key），
 * 启停走 api.mcpServer*，Key 保存走 api.setApiKeys(mcpKey)。
 */
import { ref } from "vue";
import { message } from "ant-design-vue";
import { api } from "@/api";

export function useMcpServer() {
  const mcpRunning = ref(false);
  const mcpUrl = ref("");
  const mcpPort = ref(18030);
  const mcpHost = ref("127.0.0.1");
  const mcpUseAuth = ref(false);
  const mcpKey = ref("");
  const mcpKeyInput = ref("");
  const mcpKeyVisible = ref(false);
  const mcpKeySaving = ref(false);
  const mcpToggling = ref(false);

  /** 查询运行状态 + 回填 Key */
  async function checkMcpStatus() {
    try {
      const s = await api.mcpServerStatus();
      mcpRunning.value = s.running;
      if (s.running) {
        mcpUrl.value = s.url;
        mcpPort.value = s.port;
      }
      if (s.mcp_key) mcpKey.value = s.mcp_key;
    } catch {
      mcpRunning.value = false;
    }
  }

  /** 启动 / 停止 */
  async function toggleMcpServer() {
    if (mcpToggling.value) return;
    mcpToggling.value = true;
    try {
      if (mcpRunning.value) {
        await api.mcpServerStop();
        mcpRunning.value = false;
        mcpUrl.value = "";
        message.success("MCP 服务已停止");
      } else {
        const url = await api.mcpServerStart(mcpPort.value, mcpHost.value, mcpUseAuth.value);
        mcpUrl.value = url;
        mcpRunning.value = true;
        message.success(`MCP 服务已启动：${url}`);
        if (mcpUseAuth.value) await checkMcpStatus();
      }
    } catch (e: any) {
      message.error(typeof e === "string" ? e : "操作失败");
    } finally {
      mcpToggling.value = false;
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

  function copyMcpUrl() {
    if (mcpUrl.value) copyText(mcpUrl.value);
  }

  /** 保存 MCP 专用 Key（写入 config，不影响 REST Key） */
  async function saveMcpKey() {
    const k = mcpKeyInput.value.trim();
    if (!k) return;
    mcpKeySaving.value = true;
    try {
      await api.setApiKeys(null, k);
      mcpKey.value = k;
      mcpKeyInput.value = "";
      message.success("MCP Key 已保存");
    } catch (e: any) {
      message.error(typeof e === "string" ? e : "保存失败");
    } finally {
      mcpKeySaving.value = false;
    }
  }

  return {
    mcpRunning,
    mcpUrl,
    mcpPort,
    mcpHost,
    mcpUseAuth,
    mcpKey,
    mcpKeyInput,
    mcpKeyVisible,
    mcpKeySaving,
    mcpToggling,
    checkMcpStatus,
    toggleMcpServer,
    copyMcpUrl,
    copyText,
    saveMcpKey,
  };
}
