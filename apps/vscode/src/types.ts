/**
 * byte-wiki REST API 返回的内容类型（shape 对齐 apps/desktop/src/api/types.ts）。
 * 仅声明插件用到的字段。
 */

/** 代码片段（无模板变量，纯代码） */
export interface Snippet {
  short_id: string;
  title: string;
  description: string;
  /** 代码正文 */
  code: string;
  /** 语言（用于按当前文档 languageId 过滤补全候选） */
  language: string;
  tags: string[];
}

/** 代码模板（body 是带 {{ name }} 占位符的骨架，经 Tera 渲染后插入） */
export interface Template {
  short_id: string;
  title: string;
  description: string;
  /** 模板正文（带 {{ name }} 占位符） */
  body: string;
  language: string;
  /** 触发词（可空；补全候选的触发前缀） */
  trigger: string | null;
  tags: string[];
}

/** 统一补全/选择项：把片段与模板包成同构结构，便于 QuickPick/补全混排处理 */
export interface StashItem {
  type: "snippet" | "template";
  /** 统一 id（snippet/template 的 short_id） */
  id: string;
  title: string;
  description: string;
  language: string;
  /** 片段=code；模板=body（展开前的骨架） */
  body: string;
  /** 模板才有；片段为 null */
  trigger: string | null;
}

/** 创建片段入参（对齐 REST AddSnippetB / CLI snippet add） */
export interface CreateSnippetInput {
  title: string;
  code: string;
  language?: string;
  description?: string;
  tags?: string[];
  category?: string;
}

/** 创建模板入参（对齐 REST AddTemplateB / CLI template add） */
export interface CreateTemplateInput {
  title: string;
  body: string;
  language?: string;
  trigger?: string;
  description?: string;
  tags?: string[];
  category?: string;
}

/** 导入项目入参（对齐 REST AddProjectB / CLI add） */
export interface ImportProjectInput {
  path: string;
  title?: string;
  description?: string;
  tags?: string[];
  languages?: string[];
  category?: string;
}
