<script setup lang="ts">
/**
 * 关于分区（设置 → 关于）
 *
 * 应用信息（Logo + 名称 + 版本）+ 技术栈 + 开源链接。
 * 状态在本组件内部创建（沿用既有子组件约定）。
 */
import { ref } from "vue";
import { GithubOutlined, SafetyCertificateOutlined } from "@ant-design/icons-vue";
import Logo from "@/components/common/Logo.vue";
import { api } from "@/api";

const version = ref("0.1.0");
</script>

<template>
  <section class="pane">
    <h2 class="pane-title">关于</h2>

    <!-- 应用信息 -->
    <div class="storage-section about-head-section">
      <div class="about-head">
        <Logo :size="48" color="var(--color-primary)" />
        <div class="about-head-info">
          <div class="about-name">byte-wiki</div>
          <div class="about-sub">v{{ version }}</div>
        </div>
      </div>
      <p class="hint">
        纯本地、单人、零部署的知识库——管理项目、片段、笔记，内嵌 MCP 服务器供 AI 检索。
      </p>
    </div>

    <!-- 技术栈 -->
    <div class="storage-section">
      <div class="section-head">技术栈</div>
      <div class="about-tech-row">
        <span class="about-tech-item">Rust</span>
        <span class="about-tech-item">Tauri v2</span>
        <span class="about-tech-item">Vue 3</span>
        <span class="about-tech-item">SQLite + FTS5</span>
        <span class="about-tech-item">Monaco Editor</span>
      </div>
    </div>

    <!-- 开源 -->
    <div class="storage-section about-copyright">
      <div class="about-links">
        <a class="about-link" @click="api.openUrl('https://github.com/cicbyte/byte-wiki')">
          <GithubOutlined /> 源代码
        </a>
        <a class="about-link" @click="api.openUrl('https://github.com/cicbyte/byte-wiki/blob/main/LICENSE')">
          <SafetyCertificateOutlined /> MIT License
        </a>
      </div>
      <p class="hint about-footer-hint">© 2026 byte-wiki</p>
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

/* ============ 关于页（沿用 storage-section 布局，保持风格一致）============ */
.about-head-section {
  padding-bottom: var(--spacing-md);
}
.about-head {
  display: flex;
  align-items: center;
  gap: 14px;
  margin-bottom: 12px;
}
.about-head-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.about-name {
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: -0.3px;
}
.about-sub {
  font-size: 12px;
  color: var(--text-tertiary);
  font-family: "JetBrains Mono", monospace;
}
.about-tech-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.about-tech-item {
  font-size: 12px;
  font-family: "JetBrains Mono", monospace;
  color: var(--text-secondary);
  background: var(--bg-input);
  padding: 3px 10px;
  border-radius: var(--radius-xs);
}
.about-links {
  display: flex;
  gap: var(--spacing-md);
  margin-bottom: 8px;
}
.about-link {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--color-primary);
  text-decoration: none;
  cursor: pointer;
  transition: opacity var(--transition-fast);
}
.about-link:hover {
  opacity: 0.75;
}
.about-copyright {
  border-bottom: none;
}
.about-footer-hint {
  margin: 0;
}
</style>
