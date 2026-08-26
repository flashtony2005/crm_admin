-- Ghost 化 CMS 改造 · 独立 Tag 管理迁移
-- 目标：新增 tags 表（描述/封面/SEO 字段），支撑内容组织专业化；
--       并为 articles 补 slug 列（语义化 URL 缺失列）。
-- 说明：CREATE TABLE IF NOT EXISTS / ALTER 幂等，可重复执行。
-- 执行对象：server/cms.db 与 server/cms_live.db（当前运行库）

CREATE TABLE IF NOT EXISTS tags (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL DEFAULT 't_demo',
  name TEXT NOT NULL,
  slug TEXT DEFAULT '',
  description TEXT DEFAULT '',
  cover_image TEXT,
  meta_title TEXT,
  meta_description TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tags_tenant ON tags(tenant_id);

-- articles 补 slug 列（历史建表缺失，公开 API / sitemap / RSS 依赖）
ALTER TABLE articles ADD COLUMN slug TEXT;
-- 清理历史脏标签（旧前端曾以 JSON 数组字符串存储 tags）
UPDATE articles SET tags = '' WHERE tags = '[]' OR tags IS NULL;
DELETE FROM tags WHERE name = '[]' OR name = '' OR name IS NULL;
