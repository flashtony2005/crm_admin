-- Ghost 化 CMS 改造 · 内容模型迁移
-- 目标：articles 增加封面图与计划发布时间（定时发布元数据）
-- 说明：SQLite 的 ALTER TABLE ADD COLUMN 为 O(1) 元数据操作，安全可重复（重复执行报 duplicate column 属正常）。
-- 执行对象：server/cms.db 与 server/cms_live.db（当前运行库）

ALTER TABLE articles ADD COLUMN featured_image TEXT;
ALTER TABLE articles ADD COLUMN published_at TEXT;

-- 可选：pages 同步增加 featured_image（静态页面封面）
-- ALTER TABLE pages ADD COLUMN featured_image TEXT;
