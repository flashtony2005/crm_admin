export interface Hotspot {
  keyword: string;
  score: number;
  trend: 'rising' | 'falling' | 'stable';
  source: string;
  timestamp: string;
}

export interface ArticleDraft {
  id?: string;
  title: string;
  content: string;
  tags: string[];
  author: string;
  publishDate?: string;
  hotspots: Hotspot[];
  status: 'draft' | 'scheduled' | 'published' | 'archived';
  createdAt: string;
  updatedAt: string;
  workflowId?: string;
}

export interface ArticleGenerationConfig {
  templateId: string;
  tone: 'professional' | 'casual' | 'technical' | 'persuasive';
  length: 'short' | 'medium' | 'long';
  language: string;
  includeImages?: boolean;
  seoOptimized?: boolean;
}

export interface CrawlerConfig {
  url: string;
  frequencyMs: number;
  selectors: string[];
  maxSizeKB: number;
  enabled: boolean;
}
