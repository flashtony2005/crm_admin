/**
 * CMS 演示种子数据 —— 以「山茶烘焙坊」为例的小企业内容。
 * 仅在本地集合首次初始化时写入（见 store.ts）。
 * 原始数据只写业务字段，时间戳由 withBase() 统一补齐。
 */
import type {
  AiTask, Approval, Article, Customer, FormDef, Integration,
  Lead, MediaItem, Page, Product, WorkflowDef,
} from './types'

const daysAgo = (n: number, h = 9): string => {
  const d = new Date()
  d.setDate(d.getDate() - n)
  d.setHours(h, 24 - n % 40, 0, 0)
  return d.toISOString()
}

/** 给种子记录补齐 BaseRecord 时间戳（记录自身携带的值优先） */
function withBase<T extends { id: string }>(rows: T[]): (T & { createdAt: string; updatedAt: string })[] {
  return rows.map((r, i) => ({
    createdAt: daysAgo(14 - i),
    updatedAt: daysAgo(Math.max(0, 3 - i % 4)),
    ...r,
  }))
}

// ── Content ──────────────────────────────────────────

const rawArticles: Omit<Article, 'createdAt' | 'updatedAt'>[] = [
  { id: 'a1', title: '秋季限定：桂花栗子欧包上市', slug: 'autumn-chestnut-bread', summary: '当季板栗与头茬桂花，每日限量 30 个。', content: '当季板栗与头茬桂花，每日限量 30 个。趁热撕开，桂花的清甜混着栗子的绵密，是秋天该有的味道。', category: '新品动态', status: 'published', views: 1284, author: '林小茶' },
  { id: 'a2', title: '为什么我们的面包要发酵 18 小时', slug: '18-hour-fermentation', summary: '慢发酵带来更好的风味与更友好的肠胃体验。', content: '慢发酵带来更好的风味与更友好的肠胃体验。低温长时间发酵让面筋更舒展，麦香更突出，也更易消化。', category: '品牌故事', status: 'published', views: 3521, author: '陈师傅' },
  { id: 'a3', title: '周末亲子烘焙课报名开启', slug: 'family-baking-class', summary: '每周六上午，和孩子一起做一炉小饼干。', content: '每周六上午，和孩子一起做一炉小饼干。揉面、造型、等待出炉，把周末过成甜的。', category: '活动公告', status: 'pending_review', views: 0, author: '林小茶' },
  { id: 'a4', title: '社区咖啡节回顾：我们卖了 800 杯拿铁', slug: 'coffee-festival-recap', summary: '两天的咖啡节，认识了很多街坊邻居。', content: '两天的咖啡节，认识了很多街坊邻居。谢谢每一位来打卡的你，我们明年见。', category: '门店动态', status: 'draft', views: 0, author: '周周' },
]

export const seedArticles: Article[] = withBase(rawArticles)

const rawPages: Omit<Page, 'createdAt' | 'updatedAt'>[] = [
  { id: 'p1', title: '首页', path: '/', status: 'published', views: 15230 },
  { id: 'p2', title: '关于我们', path: '/about', status: 'published', views: 3140 },
  { id: 'p3', title: '菜单', path: '/menu', status: 'published', views: 8902 },
  { id: 'p4', title: '烘焙课程', path: '/classes', status: 'draft', views: 0 },
]

export const seedPages: Page[] = withBase(rawPages)

const rawProducts: Omit<Product, 'createdAt' | 'updatedAt'>[] = [
  { id: 'pr1', name: '桂花栗子欧包', sku: 'BREAD-001', price: 28, stock: 26, status: 'published' },
  { id: 'pr2', name: '经典可颂（4 只装）', sku: 'BREAD-002', price: 36, stock: 48, status: 'published' },
  { id: 'pr3', name: '山茶拿铁豆（250g）', sku: 'BEAN-001', price: 68, stock: 15, status: 'published' },
  { id: 'pr4', name: '亲子烘焙课（单次）', sku: 'CLASS-001', price: 199, stock: 8, status: 'draft' },
]

export const seedProducts: Product[] = withBase(rawProducts)

const rawMedia: Omit<MediaItem, 'createdAt' | 'updatedAt'>[] = [
  { id: 'm1', name: '桂花栗子欧包-主图.jpg', type: 'image', sizeKb: 842, url: '#' },
  { id: 'm2', name: '门店内景-秋日.jpg', type: 'image', sizeKb: 1204, url: '#' },
  { id: 'm3', name: '可颂横切面.mp4', type: 'video', sizeKb: 15360, url: '#' },
  { id: 'm4', name: '课程海报-11月.png', type: 'image', sizeKb: 656, url: '#' },
]

export const seedMedia: MediaItem[] = withBase(rawMedia)

// ── Business ─────────────────────────────────────────

const rawCustomers: Omit<Customer, 'createdAt' | 'updatedAt'>[] = [
  { id: 'c1', name: '王女士', phone: '138****6621', source: '到店扫码', tags: ['常客', '烘焙课'], priority: 'high', note: '对亲子课程很感兴趣，想约周六上午。', lastContactAt: daysAgo(0, 10) },
  { id: 'c2', name: '李先生', phone: '139****0877', source: '小程序', tags: ['企业采购'], priority: 'high', note: '公司下午茶意向单 200 份。', lastContactAt: daysAgo(1) },
  { id: 'c3', name: '赵小姐', phone: '137****3345', source: '大众点评', tags: ['新客'], priority: 'mid', note: '询问生日蛋糕定制。', lastContactAt: daysAgo(2) },
  { id: 'c4', name: '陈阿姨', phone: '135****9902', source: '老客推荐', tags: ['常客'], priority: 'low', note: '每周固定买两个可颂。', lastContactAt: daysAgo(5) },
]

export const seedCustomers: Customer[] = withBase(rawCustomers)

const rawLeads: Omit<Lead, 'createdAt' | 'updatedAt'>[] = [
  { id: 'l1', name: '刘经理（写字楼团购）', phone: '150****2231', interest: '企业下午茶 100 人份', source: '表单', status: 'new' },
  { id: 'l2', name: '孙老师（幼儿园）', phone: '158****7742', interest: '亲子活动合作 30 组', source: '微信', status: 'following' },
  { id: 'l3', name: '吴先生', phone: '186****5560', interest: '婚礼伴手礼 80 盒', source: '大众点评', status: 'won' },
  { id: 'l4', name: '郑女士', phone: '133****8814', interest: '生日蛋糕定制', source: '小红书', status: 'lost' },
]

export const seedLeads: Lead[] = withBase(rawLeads)

const rawForms: Omit<FormDef, 'createdAt' | 'updatedAt'>[] = [
  { id: 'f1', title: '烘焙课预约表单', descr: '留下联系方式，我们 1 小时内致电确认。', fieldCount: 6, submissions: 42, status: 'published' },
  { id: 'f2', title: '企业采购询价表单', descr: '团购/采购需求，两个工作日内报价。', fieldCount: 8, submissions: 17, status: 'published' },
  { id: 'f3', title: '会员卡开通登记', fieldCount: 5, submissions: 128, status: 'closed' },
]

export const seedForms: FormDef[] = withBase(rawForms)

// ── AI / Automation ──────────────────────────────────

const rawApprovals: Omit<Approval, 'createdAt' | 'updatedAt'>[] = [
  { id: 'ap1', action: 'publish', target: '文章《周末亲子烘焙课报名开启》', requestedBy: 'AI 助手', risk: 'low', status: 'pending', summary: 'AI 已完成 SEO 检查与错别字校对，等待发布批准。' },
  { id: 'ap2', action: 'publish', target: '产品「亲子烘焙课（单次）」', requestedBy: '林小茶', risk: 'mid', status: 'pending', summary: '价格与库存已确认，上架后同步到小程序。' },
  { id: 'ap3', action: 'update', target: '页面「关于我们」', requestedBy: 'AI 助手', risk: 'low', status: 'approved', summary: '更新营业时间与门店照片，已按时执行。', decidedAt: daysAgo(2, 14) },
]

export const seedApprovals: Approval[] = withBase(rawApprovals)

const rawAiTasks: Omit<AiTask, 'createdAt' | 'updatedAt'>[] = [
  { id: 't1', title: '为《秋季限定》生成小红书文案', capability: 'Marketing Copy', status: 'done', result: '已生成 3 版文案，保存在草稿箱。' },
  { id: 't2', title: '把「烘焙课程」页面翻译成英文', capability: 'Translation', status: 'running' },
  { id: 't3', title: '汇总本周高优先级客户并给出回复建议', capability: 'Customer Reply', status: 'waiting_approval' },
  { id: 't4', title: '检查全站图片缺失 alt 标签', capability: 'SEO', status: 'failed', result: '2 张图片缺 alt，已生成修复建议。' },
]

export const seedAiTasks: AiTask[] = withBase(rawAiTasks)

const rawWorkflows: Omit<WorkflowDef, 'createdAt' | 'updatedAt'>[] = [
  { id: 'w1', name: '新客欢迎邮件', trigger: '客户创建时', event: 'customer.created', stepCount: 3, enabled: true, lastRunAt: daysAgo(1) },
  { id: 'w2', name: '每周经营摘要推送老板', trigger: '每周一 08:00', event: 'schedule.weekly', stepCount: 4, enabled: true, lastRunAt: daysAgo(3) },
  { id: 'w3', name: '线索 7 天未跟进自动提醒', trigger: '线索状态变更时', event: 'lead.stale', stepCount: 2, enabled: false },
]

export const seedWorkflows: WorkflowDef[] = withBase(rawWorkflows)

const rawIntegrations: Omit<Integration, 'createdAt' | 'updatedAt'>[] = [
  { id: 'ga', key: 'ga', name: 'Google Analytics', category: 'analytics', desc: '网站流量与转化分析', connected: true },
  { id: 'gsc', key: 'gsc', name: 'Search Console', category: 'seo', desc: '搜索引擎收录与关键词', connected: true },
  { id: 'email', key: 'email', name: 'Email 营销', category: 'message', desc: '欢迎邮件、活动群发', connected: true },
  { id: 'wecom-bot', key: 'wecom-bot', name: '企业微信群机器人', category: 'message', desc: '粘贴 Webhook 地址即可收到审批推送', connected: false },
  { id: 'wechat', key: 'wechat', name: '微信公众号', category: 'message', desc: '推文同步与模板消息', connected: false },
  { id: 'wecom', key: 'wecom', name: '企业微信', category: 'message', desc: '客户联系与审批通知', connected: false },
  { id: 'stripe', key: 'stripe', name: 'Stripe', category: 'commerce', desc: '线上收款与订阅', connected: false },
  { id: 'shopify', key: 'shopify', name: 'Shopify', category: 'commerce', desc: '商品与订单双向同步', connected: false },
  { id: 'crm', key: 'crm', name: 'CRM 导出', category: 'crm', desc: '客户数据定期导出', connected: false },
]

export const seedIntegrations: Integration[] = withBase(rawIntegrations)
