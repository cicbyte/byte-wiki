/** 前端类型定义（镜像 byte-wiki-core 的 model） */

export interface Project {
  short_id: string;
  title: string;
  description: string;
  category_id: number | null;
  status: "normal" | "archived";
  is_favorited: boolean;
  view_count: number;
  created_at: string;
  updated_at: string;
  source_path: string | null;
  tags: string[];
  languages: string[];
  category: string | null;
}

export interface StatsOverview {
  project_count: number;
  snippet_count: number;
  note_count: number;
  diagram_count: number;
  template_count: number;
  tag_count: number;
  language_count: number;
  category_count: number;
  total_size: number;
  favorited_count: number;
  /** 回收站内条目数（三类合计） */
  trashed_count: number;
}

export interface Snippet {
  short_id: string;
  title: string;
  description: string;
  code: string;
  language: string;
  category_id: number | null;
  category: string | null;
  is_favorited: boolean;
  view_count: number;
  created_at: string;
  updated_at: string;
  tags: string[];
}

export interface SnippetSearchHit {
  short_id: string;
  title: string;
  language: string;
  snippet: string;
  rank: number;
}

export interface Note {
  short_id: string;
  title: string;
  description: string;
  content: string;
  category_id: number | null;
  category: string | null;
  is_favorited: boolean;
  view_count: number;
  created_at: string;
  updated_at: string;
  tags: string[];
}

export interface NoteSearchHit {
  short_id: string;
  title: string;
  snippet: string;
  rank: number;
}

export interface Diagram {
  short_id: string;
  title: string;
  description: string;
  /** 源码（mermaid/svg，从文件读取） */
  source: string;
  /** 图表类型："mermaid" | "svg" */
  kind: string;
  category_id: number | null;
  category: string | null;
  is_favorited: boolean;
  view_count: number;
  created_at: string;
  updated_at: string;
  tags: string[];
}

export interface DiagramSearchHit {
  short_id: string;
  title: string;
  kind: string;
  snippet: string;
  rank: number;
}

export interface Template {
  short_id: string;
  title: string;
  description: string;
  /** 模板正文（带 ${name} 占位符的骨架，从文件读取） */
  body: string;
  /** 模板适用语言（决定扩展名 + 高亮） */
  language: string;
  /** 触发词（编辑器补全用，可空） */
  trigger: string | null;
  category_id: number | null;
  category: string | null;
  is_favorited: boolean;
  view_count: number;
  created_at: string;
  updated_at: string;
  tags: string[];
}

export interface TemplateSearchHit {
  short_id: string;
  title: string;
  language: string;
  snippet: string;
  rank: number;
}

export interface Category {
  id: number;
  name: string;
  project_count: number;
  snippet_count: number;
  note_count: number;
  diagram_count: number;
  template_count: number;
}

export interface SearchHit {
  short_id: string;
  title: string;
  snippet: string;
  rank: number;
  matched_tags: string[];
}

/** 统一搜索命中（跨 project/snippet/note 三类合并） */
export interface UnifiedSearchHit {
  short_id: string;
  item_type: ItemType;
  title: string;
  /** 命中上下文（FTS 高亮片段 / LIKE 兜底时为空） */
  snippet: string;
  category: string | null;
  /** 相关度（FTS bm25 负数越小越相关；LIKE 兜底为 0） */
  rank: number;
}

export interface FileNode {
  name: string;
  path: string;
  children?: FileNode[];
  size: number;
}

export interface TagStat {
  tag: string;
  count: number;
}

export interface LanguageStat {
  language: string;
  /** 总使用次数 = project_count + snippet_count */
  count: number;
  project_count: number;
  snippet_count: number;
}

export interface ListParams {
  tag?: string;
  language?: string;
  category?: string;
  favorited_only?: boolean;
  limit?: number;
  offset?: number;
}

/** 分页结果（list_*_paged 命令返回） */
export interface PagedResult<T> {
  items: T[];
  total: number;
}

// ============ 回收站 ============

export type ItemType = "project" | "snippet" | "note" | "diagram" | "template";

export interface TrashedItem {
  short_id: string;
  item_type: ItemType;
  title: string;
  description: string;
  category: string | null;
  /** 删除时间（ISO8601） */
  deleted_at: string;
  /** 创建时间（ISO8601） */
  created_at: string;
}

// ============ 分享 ============

/** 分享类型（与 Rust ItemType 对齐） */
export type ShareKind = "project" | "snippet" | "note" | "diagram" | "template";

/** 分享令牌记录 */
export interface Share {
  token: string;
  item_type: ShareKind;
  item_id: string;
  created_at: string;
  /** 过期时间（ISO8601）；null = 永久 */
  expires_at: string | null;
  view_count: number;
  last_viewed_at: string | null;
}

// ============ 资产 ============

/** 图片资产（内容寻址 md5） */
export interface Asset {
  /** 文件内容 MD5（主键） */
  md5: string;
  /** 原扩展名（png/jpg/...） */
  ext: string;
  /** 原图字节数 */
  size: number;
  /** 原图宽（解析失败时字段省略） */
  width?: number;
  height?: number;
  /** 首次上传的原始文件名 */
  original_name: string;
  /** 是否已生成缩略图 */
  thumb_exists: boolean;
  created_at: string;
  /** 软删除时间；正常态字段省略，回收站态有值 */
  deleted_at?: string | null;
}

/** 资产引用方（哪个实体引用了该资产） */
export interface AssetReferrer {
  item_type: ItemType;
  short_id: string;
  title: string;
}

/** 资产引用扫描结果 */
export interface AssetReference {
  md5: string;
  referrers: AssetReferrer[];
}
