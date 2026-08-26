-- Ghost 化 CMS 改造 · 文章级 SEO 字段迁移
-- 目标：articles 增加独立 SEO 标题与描述（meta_title / meta_description），
--       供公开站 HTML <head> 的 title/description/OG/Twitter 使用。
-- 说明：SQLite 的 ALTER TABLE ADD COLUMN 为 O(1) 元数据操作，安全可重复（重复执行报 duplicate column 属正常）。
-- 执行对象：server/cms.db 与 server/cms_live.db（当前运行库）

ALTER TABLE articles ADD COLUMN meta_title TEXT;
ALTER TABLE articles ADD COLUMN meta_description TEXT;
