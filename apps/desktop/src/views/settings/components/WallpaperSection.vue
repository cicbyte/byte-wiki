<script setup lang="ts">
/**
 * 壁纸设置分区（外观页内，对齐 ax 面板级毛玻璃）
 *
 * - 上传图片（复制到 data/wallpapers/）→ 自动设为当前
 * - 壁纸库缩略图网格：点选切换、hover 删除
 * - 模糊 / 遮罩 / 面板透明度 三档滑块（store apply 注入 --wp-blur / --wp-panel-alpha）
 * - 清除壁纸（恢复纯色面板）
 */
import { ref, onMounted } from "vue";
import { message } from "ant-design-vue";
import { open } from "@tauri-apps/plugin-dialog";
import { PlusOutlined, DeleteOutlined } from "@ant-design/icons-vue";
import { useWallpaperStore } from "@/stores/wallpaper";
import { api } from "@/api";

const wallpaper = useWallpaperStore();
const uploading = ref(false);

async function onUpload() {
  uploading.value = true;
  try {
    const selected = await open({
      filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp", "avif"] }],
      multiple: false,
    });
    if (!selected) return;
    const filename = await api.wallpaperUpload(selected as string);
    await wallpaper.refreshList();
    wallpaper.setActive(filename);
    message.success("壁纸已设置");
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "上传失败");
  } finally {
    uploading.value = false;
  }
}

function onPick(filename: string) {
  wallpaper.setActive(filename);
}

async function onDelete(filename: string) {
  try {
    if (wallpaper.activeName === filename) wallpaper.clear();
    await api.wallpaperDelete(filename);
    await wallpaper.refreshList();
    message.success("已删除");
  } catch (e: any) {
    message.error(typeof e === "string" ? e : "删除失败");
  }
}

onMounted(async () => {
  // 确保 dataDir 就绪（缩略图 URL 依赖它）
  if (!wallpaper.dataDir) await wallpaper.init();
});
</script>

<template>
  <div class="wallpaper-section">
    <a-divider orientation="left" plain>壁纸</a-divider>
    <div class="wp-enable-row">
      <a-switch
        :checked="wallpaper.enabled"
        :disabled="!wallpaper.hasWallpaper"
        size="small"
        @change="wallpaper.setEnabled"
      />
      <span class="wp-enable-label">启用壁纸</span>
      <span class="wp-hint">关闭后恢复纯色面板，已上传壁纸保留。</span>
    </div>
    <a-alert type="info" show-icon class="wp-tip">
      <template #message>壁纸说明</template>
      <template #description>
        <ul class="wp-tip-list">
          <li>推荐比例 <b>16:9</b>（横屏，匹配宽屏窗口）；分辨率 <b>1920×1080</b> 或更高更清晰。</li>
          <li>支持格式：PNG / JPG / WebP / GIF / BMP / AVIF。</li>
          <li>壁纸作为全窗口背景，顶栏/侧栏/底栏/卡片按「面板透明度」各自毛玻璃半透明；调高「遮罩」可保证文字可读。</li>
          <li>图片存于数据目录 <code>wallpapers/</code>，随备份保存。</li>
          <li>启用壁纸时推荐使用<b>自定义主题</b>，以便精细调整面板/文字颜色，保证可读性。</li>
        </ul>
      </template>
    </a-alert>
    <div class="wp-grid">
      <div class="wp-tile wp-upload" :class="{ busy: uploading }" @click="onUpload">
        <PlusOutlined />
        <span>{{ uploading ? "上传中…" : "上传" }}</span>
      </div>
      <div
        v-for="f in wallpaper.list"
        :key="f"
        class="wp-tile wp-thumb"
        :class="{ active: wallpaper.activeName === f }"
        :style="{ backgroundImage: `url('${wallpaper.thumbUrl(f)}')` }"
        :title="f"
        @click="onPick(f)"
      >
        <button class="wp-del" title="删除" @click.stop="onDelete(f)"><DeleteOutlined /></button>
      </div>
    </div>

    <div class="wp-controls">
      <div class="wp-row">
        <span class="wp-label">模糊度</span>
        <a-slider
          :value="wallpaper.blur"
          :min="0"
          :max="40"
          :step="1"
          :disabled="!wallpaper.hasWallpaper || !wallpaper.enabled"
          class="wp-slider"
          @change="(v: number) => wallpaper.setBlur(v)"
        />
        <span class="wp-val">{{ wallpaper.blur }}px</span>
      </div>
      <div class="wp-row">
        <span class="wp-label">遮罩浓度</span>
        <a-slider
          :value="Math.round(wallpaper.mask * 100)"
          :min="0"
          :max="80"
          :step="1"
          :disabled="!wallpaper.hasWallpaper || !wallpaper.enabled"
          class="wp-slider"
          @change="(v: number) => wallpaper.setMask(v / 100)"
        />
        <span class="wp-val">{{ Math.round(wallpaper.mask * 100) }}%</span>
      </div>
      <div class="wp-row">
        <span class="wp-label">面板透明度</span>
        <a-slider
          :value="Math.round(wallpaper.panelAlpha * 100)"
          :min="0"
          :max="70"
          :step="1"
          :disabled="!wallpaper.hasWallpaper || !wallpaper.enabled"
          class="wp-slider"
          @change="(v: number) => wallpaper.setPanelAlpha(v / 100)"
        />
        <span class="wp-val">{{ Math.round(wallpaper.panelAlpha * 100) }}%</span>
      </div>
    </div>

    <div class="wp-actions">
      <span class="wp-hint">无壁纸时面板恢复纯色。</span>
      <a-button v-if="wallpaper.hasWallpaper" size="small" @click="wallpaper.clear()">清除壁纸</a-button>
    </div>
  </div>
</template>

<style scoped>
.wallpaper-section { margin-top: 24px; }
.wp-enable-row { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
.wp-enable-label { font-size: 13px; font-weight: 600; color: var(--text-secondary); }
.wp-tip { margin-bottom: 16px; }
.wp-tip-list { margin: 0; padding-left: 18px; display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--text-secondary); }
.wp-tip-list code { font-family: "JetBrains Mono", monospace; background: var(--bg-input); padding: 0 4px; border-radius: 3px; font-size: 11px; }
.wp-grid { display: flex; flex-wrap: wrap; gap: 10px; }
.wp-tile {
  width: 96px;
  height: 60px;
  border: 1px solid var(--border-card);
  border-radius: 6px;
  overflow: hidden;
  position: relative;
  cursor: pointer;
  background-size: cover;
  background-position: center;
  transition: all var(--transition-fast);
}
.wp-tile:hover { transform: translateY(-1px); border-color: var(--color-primary-border); }
.wp-tile.active { box-shadow: 0 0 0 2px var(--color-primary); }
.wp-upload {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  color: var(--text-tertiary);
  font-size: 12px;
  border-style: dashed;
}
.wp-upload.busy { opacity: 0.6; cursor: wait; }
.wp-del {
  position: absolute;
  top: 2px;
  right: 2px;
  width: 20px;
  height: 20px;
  border: none;
  border-radius: 4px;
  background: rgba(0, 0, 0, 0.55);
  color: #fff;
  cursor: pointer;
  font-size: 11px;
  display: none;
  align-items: center;
  justify-content: center;
}
.wp-thumb:hover .wp-del { display: flex; }

.wp-controls { margin-top: 16px; display: flex; flex-direction: column; gap: 6px; }
.wp-row { display: flex; align-items: center; gap: 12px; }
.wp-label { width: 84px; font-size: 13px; color: var(--text-secondary); flex-shrink: 0; }
.wp-slider { flex: 1; min-width: 0; }
.wp-val { width: 44px; text-align: right; font-size: 12px; color: var(--text-tertiary); font-family: "JetBrains Mono", monospace; flex-shrink: 0; }

.wp-actions { margin-top: 12px; display: flex; align-items: center; gap: 12px; }
.wp-hint { flex: 1; font-size: 11px; color: var(--text-tertiary); line-height: 1.6; }
</style>
