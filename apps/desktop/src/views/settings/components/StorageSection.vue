<script setup lang="ts">
/**
 * 数据分区（设置 → 数据）
 *
 * - 数据目录：显示 / 在文件管理器打开 / 更改目录（写入指针文件，提示重启）
 * - 备份与恢复：导出 zip / 导入恢复（覆盖，提示重启）
 * - 同步建议说明
 *
 * 状态在本组件内部创建（沿用既有子组件约定：AppearanceForm / WallpaperSection 自管状态 + 自调 API）。
 */
import { ref, onMounted } from "vue";
import {
  FolderOpenOutlined,
  FolderViewOutlined,
  DownloadOutlined,
  UploadOutlined,
} from "@ant-design/icons-vue";
import { message, Modal } from "ant-design-vue";
import { api } from "@/api";

const dataDir = ref("");
const backing = ref(false);
const restoring = ref(false);

onMounted(async () => {
  try {
    dataDir.value = await api.dataDir();
  } catch {
    dataDir.value = "(未知)";
  }
});

/** 选择新数据目录（写入指针文件，提示重启） */
async function chooseDataDir() {
  const dir = await api.pickDirectory();
  if (!dir) return;
  Modal.confirm({
    title: "更改数据目录",
    content: "新目录：" + dir + "\n\n点击确定后需要重启 byte-wiki 使新目录生效。当前数据不会自动迁移，如需迁移请先用「导出备份」。",
    okText: "确定，重启后生效",
    cancelText: "取消",
    onOk: async () => {
      await api.setDataDir(dir);
      message.success("已设置，请重启 byte-wiki");
    },
  });
}

/** 在系统文件管理器中打开当前数据目录 */
async function openDataDir() {
  if (!dataDir.value) {
    message.warning("数据目录尚未加载");
    return;
  }
  try {
    await api.openInFolder(dataDir.value);
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "打开失败");
  }
}

/** 导出备份（先弹保存对话框选择导出路径，再导出到该路径） */
async function exportBackup() {
  const target = await api.pickSavePath();
  if (!target) return; // 用户在保存对话框取消
  backing.value = true;
  try {
    const out = await api.backup(target);
    message.success("已导出：" + (out || target));
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "导出失败");
  } finally {
    backing.value = false;
  }
}

/** 导入备份（选择文件→确认→恢复→提示重启） */
async function importBackup() {
  const file = await api.pickFile();
  if (!file) return;
  Modal.confirm({
    title: "恢复备份",
    content: "将从 " + file + " 恢复数据，当前数据会被覆盖。恢复后需要重启 byte-wiki。",
    okText: "确定恢复",
    okType: "danger",
    cancelText: "取消",
    onOk: async () => {
      restoring.value = true;
      try {
        await api.restore(file);
        message.success("已恢复，请重启 byte-wiki");
      } catch (e: any) {
        message.error(typeof e === "string" ? e : "恢复失败");
      } finally {
        restoring.value = false;
      }
    },
  });
}
</script>

<template>
  <section class="pane">
    <h2 class="pane-title">数据</h2>

    <!-- 数据目录 -->
    <div class="storage-section">
      <div class="section-head">数据目录</div>
      <div class="dir-row">
        <span class="field-value mono dir-path">{{ dataDir }}</span>
        <button class="storage-btn" @click="openDataDir" title="在文件管理器中打开">
          <FolderViewOutlined /> 打开目录
        </button>
        <button class="storage-btn" @click="chooseDataDir" title="选择其他数据目录">
          <FolderOpenOutlined /> 更改目录
        </button>
      </div>
      <p class="hint">
        所有数据（数据库、项目文件、片段、笔记）都在这一个目录下。更改后需重启生效，当前数据不会自动迁移。
      </p>
    </div>

    <!-- 备份与恢复 -->
    <div class="storage-section">
      <div class="section-head">备份与恢复</div>
      <div class="backup-actions">
        <button class="storage-btn primary" :disabled="backing" @click="exportBackup">
          <DownloadOutlined /> {{ backing ? "导出中…" : "导出备份" }}
        </button>
        <button class="storage-btn" :disabled="restoring" @click="importBackup">
          <UploadOutlined /> {{ restoring ? "恢复中…" : "导入备份" }}
        </button>
      </div>
      <p class="hint">
        导出会把数据库和全部内容打包为 zip（含 WAL 一致性检查）。恢复会覆盖当前数据，操作后需重启。
      </p>
    </div>

    <!-- 同步建议 -->
    <div class="storage-section">
      <div class="section-head">同步与外部备份</div>
      <p class="hint">
        也可以直接用文件同步工具（OneDrive / 坚果云 / Syncthing）同步数据目录，
        或用 git 管理。CLI 提供 <code>bs backup</code> / <code>bs restore</code> 命令供脚本自动化。
      </p>
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

/* 数据分区 */
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
.dir-row {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
}
.dir-path {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: var(--text-secondary);
  background: var(--bg-input);
  padding: 6px 10px;
  border-radius: var(--radius-sm);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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

.field-row {
  display: flex;
  gap: 12px;
  padding: 6px 0;
  font-size: 13px;
  align-items: center;
}
.field-label {
  color: var(--text-tertiary);
  min-width: 80px;
}
.field-value {
  color: var(--text-primary);
}
.mono {
  font-family: "JetBrains Mono", monospace;
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
.code-block {
  background: var(--bg-input);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-md);
  padding: 10px 12px;
  font-family: "JetBrains Mono", monospace;
  font-size: 12px;
  color: var(--text-primary);
  overflow-x: auto;
  margin-top: 12px;
}
</style>
