<script setup lang="ts">
/**
 * 帮助面板（设置 → 帮助）
 * 左：帮助文档目录树（按分类分组）；右：选中文档的 Markdown 详情。
 * 渲染统一用 md-editor-v3 的 MdPreview。
 */
import { ref, computed } from "vue";
import { MdPreview } from "md-editor-v3";
import { useThemeStore } from "@/stores/theme";

/** 帮助文档：key / 标题 / 分类 / Markdown 正文 */
interface HelpDoc {
  key: string;
  title: string;
  group: string;
  md: string;
}

const TEMPLATE_SYNTAX_MD = `# 模板引擎语法

模板正文使用 **Tera**（Jinja2 子集）语法。在模板里用占位符声明变量，「使用模板」时弹出对话框逐个填值，渲染后插入到代码编辑器（或复制到剪贴板）。

## 变量

双花括号包裹变量名：

~~~
function {{ name }}({{ params }}) {
  return {{ body }};
}
~~~

填入 \`name=foo\`、\`params=a, b\`、\`body=a + b\`，渲染为：

~~~
function foo(a, b) {
  return a + b;
}
~~~

## 过滤器

用 \`|\` 应用过滤器转换值：

| 写法 | 说明 |
|---|---|
| \`{{ name | upper }}\` | 转大写 |
| \`{{ name | lower }}\` | 转小写 |
| \`{{ name | trim }}\` | 去首尾空白 |
| \`{{ name | default(value=0) }}\` | 未填时用默认值 |

## 条件 / 循环

支持 \`{% if %}\` / \`{% for %}\`：

~~~
{% if debug %}
console.log("{{ name }}");
{% endif %}

{% for item in items %}
- {{ item }}
{% endfor %}
~~~

## 触发词与展开入口

- 模板可设**触发词**（如 \`rc\`），在任意代码编辑器输入触发词会弹出补全候选，选中即展开
- 也可用 **Alt+T** 呼出命令面板搜索模板，或编辑器**右键 → 插入模板**，或模板页的**「使用」**按钮

## 注意事项

- 变量名仅支持字母 / 数字 / 下划线（对话框按此提取变量列表）
- 嵌套 \`{{ obj.field }}\`、循环变量等对话框不识别，需在模板内自行处理
- 缺失变量（既未填也无 \`default\`）会**渲染失败**并提示
- 渲染在**后端 Tera** 完成；前端预览实时调用后端（约 300ms 延迟）
`;

const docs: HelpDoc[] = [
  { key: "template-syntax", title: "模板引擎语法", group: "模板", md: TEMPLATE_SYNTAX_MD },
];

const themeStore = useThemeStore();
const selectedKey = ref(docs[0]?.key ?? "");
const selectedDoc = computed(() => docs.find((d) => d.key === selectedKey.value) ?? null);

/** 按分组组织目录（树形） */
const tree = computed(() => {
  const groups = new Map<string, HelpDoc[]>();
  for (const d of docs) {
    if (!groups.has(d.group)) groups.set(d.group, []);
    groups.get(d.group)!.push(d);
  }
  return [...groups.entries()].map(([group, items]) => ({ group, items }));
});
</script>

<template>
  <div class="help-pane">
    <!-- 左：目录树 -->
    <aside class="help-tree">
      <div class="tree-head">目录</div>
      <div v-for="g in tree" :key="g.group" class="tree-group">
        <div class="tree-group-title">{{ g.group }}</div>
        <ul class="tree-list">
          <li
            v-for="d in g.items"
            :key="d.key"
            class="tree-item"
            :class="{ active: selectedKey === d.key }"
            @click="selectedKey = d.key"
          >
            {{ d.title }}
          </li>
        </ul>
      </div>
    </aside>

    <!-- 右：Markdown 详情 -->
    <section class="help-detail">
      <MdPreview v-if="selectedDoc" :modelValue="selectedDoc.md" :theme="themeStore.isDark ? 'dark' : 'light'" class="help-md" />
      <div v-else class="help-empty">选择左侧目录查看文档</div>
    </section>
  </div>
</template>

<style scoped>
.help-pane {
  display: flex;
  height: 100%;
  min-height: 0;
  gap: 1px;
  background: var(--border-light);
}
.help-tree {
  width: 220px;
  flex-shrink: 0;
  background: var(--bg-card);
  overflow-y: auto;
  padding: 8px 0;
}
.tree-head {
  padding: 8px 16px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}
.tree-group {
  margin-bottom: 4px;
}
.tree-group-title {
  padding: 6px 16px 2px;
  font-size: 11px;
  color: var(--text-tertiary);
}
.tree-list {
  list-style: none;
  margin: 0;
  padding: 0;
}
.tree-item {
  padding: 7px 16px 7px 24px;
  font-size: 13px;
  color: var(--text-secondary);
  cursor: pointer;
  transition: background var(--transition-fast);
}
.tree-item:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}
.tree-item.active {
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 500;
}
.help-detail {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  background: var(--bg-card);
}
.help-md {
  padding: 0 20px;
}
/* 深色模式下 md-editor-v3 自带纯黑背景（--md-bk-color:#000），置透明融入卡片背景 */
.help-detail :deep(.md-editor) {
  background-color: transparent;
}
.help-empty {
  padding: 40px;
  text-align: center;
  color: var(--text-tertiary);
}
</style>
