import type { Hotspot, ArticleDraft, ArticleGenerationConfig, CrawlerConfig } from '../types/article';

export const fetchHotspots = async (source: string = 'all', limit: number = 20): Promise<Hotspot[]> => {
  const res = await fetch(`/api/articles/hotspots?source=${source}&limit=${limit}`);
  if (!res.ok) throw new Error('Failed to fetch hotspots');
  return res.json();
};

export const generateArticleFromHotspot = async (hotspotId: string, config: ArticleGenerationConfig): Promise<ArticleDraft> => {
  const res = await fetch(`/api/articles/generate?hotspot=${hotspotId}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(config),
  });
  if (!res.ok) throw new Error('Failed to generate article');
  return res.json();
};

export const getCrawlers = async (): Promise<CrawlerConfig[]> => {
  const res = await fetch('/api/crawlers');
  if (!res.ok) throw new Error('Failed to fetch crawlers');
  return res.json();
};

export const startCrawler = async (crawlerId: string) => {
  const res = await fetch(`/api/crawlers/${crawlerId}/start`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to start crawler');
  return res.json();
};

export const stopCrawler = async (crawlerId: string) => {
  const res = await fetch(`/api/crawlers/${crawlerId}/stop`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to stop crawler');
  return res.json();
};

export const listArticles = async (status?: string): Promise<ArticleDraft[]> => {
  const res = await fetch(`/api/articles${status ? `?status=${status}` : ''}`);
  if (!res.ok) throw new Error('Failed to list articles');
  return res.json();
};

export const publishArticle = async (articleId: string): Promise<ArticleDraft> => {
  const res = await fetch(`/api/articles/${articleId}/publish`, { method: 'POST' });
  if (!res.ok) throw new Error('Failed to publish article');
  return res.json();
};
