import { useCallback, useEffect, useState } from 'react'
import { createFileRoute } from '@tanstack/react-router'
import { motion, AnimatePresence } from 'framer-motion'
import type { LucideIcon } from 'lucide-react'
import {
  AlertTriangle, CheckCircle2, Check, ChevronRight, ClipboardCheck, ClipboardList,
  FileText, File, FolderOpen, Image as ImageIcon, Inbox, KeyRound, LayoutDashboard,
  LogOut, Package, Pencil, Plus, ShieldAlert, Sparkles, Trash2, TrendingUp,
  User, Users, X,
} from 'lucide-react'

/**
 * /m —— 移动工作台（多标签 App）。
 *
 * 标签：待办审批 / 概览 / 内容 / 我的
 * - 复用后端反射式网关：列表 GET /api/{table}（返回 {data,total}）、
 *   详情 GET /api/{table}/{id}、新建 POST、更新 PUT、删除 DELETE；
 * - 审批裁决 POST /api/approvals/{id}/decide（仅 Owner）；
 * - 当前用户 GET /api/user/me；
 * - 推送以 15s 轮询实现（待办标签角标），接 Web Push 属于后续增强；
 * - 权限由后端强制：无写权限的角色调用写接口会得到 403，前端对 viewer 隐藏写按钮。
 *
 * 设计：自包含、不套主布局，适合手机浏览器 / PWA 直接打开。
 * 视觉：2025 移动端设计语言 —— 渐变品牌区、毛玻璃、线性图标、柔和阴影、触摸反馈。
 */

// ── 请求封装（移动端自管理 token，避免全局 401 跳转到桌面登录页）──
async function mreq<T = any>(path: string, opts: RequestInit = {}): Promise<T & { ok?: boolean; error?: string }> {
  const token = localStorage.getItem('auth_token')
  const res = await fetch(path, {
    ...opts,
    headers: {
      'Content-Type': 'application/json',
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...((opts.headers as Record<string, string>) || {}),
    },
  })
  const body = (await res.json().catch(() => ({}))) as any
  if (res.status === 401) localStorage.removeItem('auth_token')
  if (!res.ok || body.ok === false) {
    return { ok: false, error: body.error || `请求失败 (${res.status})` } as any
  }
  return body
}

type Rec = Record<string, any>

// ── 设计令牌 ───────────────────────────────────────────
const BRAND = 'from-[#6C4DF6] via-[#7C5CFF] to-[#A06BFF]'
const INPUT =
  'h-12 px-4 rounded-2xl border border-black/[0.06] bg-[#F4F5FA] text-[15px] outline-none transition-all focus:bg-white focus:border-[#7C5CFF] focus:ring-4 focus:ring-[#7C5CFF]/10'
const CARD = 'rounded-3xl bg-white shadow-sm border border-black/[0.04]'
const BTN_PRIMARY = `h-12 rounded-2xl bg-gradient-to-r ${BRAND} text-white font-semibold text-[15px] shadow-lg shadow-[#7C5CFF]/30 active:scale-[0.98] transition-all disabled:opacity-40`

// ── 内容资源图标 / 色调（驱动列表与统计卡）─────────────
const RES_ICON: Record<string, LucideIcon> = {
  articles: FileText, pages: File, products: Package, media: ImageIcon,
  customers: Users, leads: TrendingUp, forms: ClipboardList,
}
const RES_ICON_TONE: Record<string, string> = {
  articles: 'from-violet-500 to-purple-500',
  pages: 'from-blue-500 to-indigo-500',
  products: 'from-cyan-500 to-teal-500',
  media: 'from-pink-500 to-rose-500',
  customers: 'from-amber-500 to-orange-500',
  leads: 'from-emerald-500 to-green-500',
  forms: 'from-slate-500 to-gray-500',
}

// ── 状态徽标配色 ───────────────────────────────────────────
function statusTone(s: string): string {
  switch (s) {
    case 'published':
    case 'approved':
    case 'done':
    case 'won':
      return 'bg-emerald-50 text-emerald-600'
    case 'pending':
    case 'pending_review':
    case 'following':
    case 'new':
    case 'running':
      return 'bg-amber-50 text-amber-600'
    case 'rejected':
    case 'lost':
    case 'offline':
    case 'closed':
    case 'failed':
      return 'bg-gray-100 text-gray-500'
    default:
      return 'bg-[#7C5CFF]/10 text-[#6C4DF6]'
  }
}
const STATUS_LABEL: Record<string, string> = {
  draft: '草稿', pending_review: '待审', published: '已发布', offline: '已下线',
  pending: '待处理', approved: '已批准', rejected: '已驳回',
  new: '新线索', following: '跟进中', won: '已成交', lost: '已流失',
  open: '开放', running: '进行中', done: '完成', failed: '失败',
  high: '高', mid: '中', low: '低', normal: '普通',
}
function StatusBadge({ s }: { s: string }) {
  return <span className={`text-[11px] px-2 py-0.5 rounded-lg font-medium ${statusTone(s)}`}>{STATUS_LABEL[s] ?? s}</span>
}

// 风险徽章（带图标）
function RiskBadge({ risk }: { risk?: string }) {
  const [Icon, cls, label] =
    risk === 'high'
      ? [ShieldAlert, 'bg-red-50 text-red-600', '高风险']
      : risk === 'mid'
        ? [AlertTriangle, 'bg-amber-50 text-amber-600', '中风险']
        : [CheckCircle2, 'bg-gray-100 text-gray-500', '低风险']
  return (
    <span className={`inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-lg font-medium ${cls}`}>
      <Icon size={11} strokeWidth={2.2} />{label}
    </span>
  )
}

// ── 内容资源元信息（驱动列表 / 表单）───────────────────────
interface FieldDef {
  key: string
  label: string
  type: 'text' | 'textarea' | 'number' | 'select' | 'tags' | 'bool'
  options?: { value: string; label: string }[]
  required?: boolean
}
interface ResMeta {
  key: string
  label: string
  title: (r: Rec) => string
  subtitle?: (r: Rec) => string
  statusField?: string
  fields: FieldDef[]
}
const RES: ResMeta[] = [
  {
    key: 'articles', label: '文章',
    title: (r) => r.title || '(无标题)', subtitle: (r) => r.author || '', statusField: 'status',
    fields: [
      { key: 'title', label: '标题', type: 'text', required: true },
      { key: 'summary', label: '摘要', type: 'textarea' },
      { key: 'content', label: '正文', type: 'textarea' },
      { key: 'author', label: '作者', type: 'text' },
      { key: 'status', label: '状态', type: 'select', options: [
        { value: 'draft', label: '草稿' }, { value: 'pending_review', label: '待审' },
        { value: 'published', label: '已发布' }, { value: 'offline', label: '已下线' }] },
      { key: 'tags', label: '标签(逗号分隔)', type: 'tags' },
    ],
  },
  {
    key: 'pages', label: '页面',
    title: (r) => r.title || '(无标题)', subtitle: (r) => r.path || '', statusField: 'status',
    fields: [
      { key: 'title', label: '标题', type: 'text', required: true },
      { key: 'slug', label: '路径', type: 'text' },
      { key: 'content', label: '内容', type: 'textarea' },
      { key: 'status', label: '状态', type: 'select', options: [
        { value: 'draft', label: '草稿' }, { value: 'pending_review', label: '待审' },
        { value: 'published', label: '已发布' }, { value: 'offline', label: '已下线' }] },
    ],
  },
  {
    key: 'products', label: '产品',
    title: (r) => r.name || '(未命名)', subtitle: (r) => `¥${Number(r.price || 0).toFixed(2)}`, statusField: 'status',
    fields: [
      { key: 'name', label: '名称', type: 'text', required: true },
      { key: 'price', label: '价格', type: 'number' },
      { key: 'description', label: '描述', type: 'textarea' },
      { key: 'status', label: '状态', type: 'select', options: [
        { value: 'draft', label: '草稿' }, { value: 'pending_review', label: '待审' },
        { value: 'published', label: '在售' }, { value: 'offline', label: '下架' }] },
    ],
  },
  {
    key: 'media', label: '媒体',
    title: (r) => r.name || '(未命名)', subtitle: (r) => r.url || '', statusField: 'kind',
    fields: [
      { key: 'name', label: '名称', type: 'text', required: true },
      { key: 'url', label: 'URL', type: 'text' },
      { key: 'size', label: '大小(KB)', type: 'number' },
      { key: 'kind', label: '类型', type: 'select', options: [
        { value: 'image', label: '图片' }, { value: 'video', label: '视频' }, { value: 'file', label: '文件' }] },
    ],
  },
  {
    key: 'customers', label: '客户',
    title: (r) => r.name || '(未命名)', subtitle: (r) => r.phone || '', statusField: 'priority',
    fields: [
      { key: 'name', label: '姓名', type: 'text', required: true },
      { key: 'phone', label: '电话', type: 'text' },
      { key: 'source', label: '来源', type: 'text' },
      { key: 'priority', label: '优先级', type: 'select', options: [
        { value: 'normal', label: '普通' }, { value: 'high', label: '高' }, { value: 'mid', label: '中' }, { value: 'low', label: '低' }] },
      { key: 'note', label: '备注', type: 'textarea' },
      { key: 'tags', label: '标签(逗号分隔)', type: 'tags' },
    ],
  },
  {
    key: 'leads', label: '线索',
    title: (r) => r.name || '(未命名)', subtitle: (r) => r.interest || '', statusField: 'status',
    fields: [
      { key: 'name', label: '姓名', type: 'text', required: true },
      { key: 'phone', label: '电话', type: 'text' },
      { key: 'interest', label: '意向', type: 'text' },
      { key: 'source', label: '来源', type: 'text' },
      { key: 'status', label: '状态', type: 'select', options: [
        { value: 'new', label: '新线索' }, { value: 'following', label: '跟进中' },
        { value: 'won', label: '已成交' }, { value: 'lost', label: '已流失' }] },
    ],
  },
  {
    key: 'forms', label: '表单',
    title: (r) => r.title || '(未命名)', subtitle: (r) => `${r.submissions || 0} 份提交`, statusField: 'status',
    fields: [
      { key: 'title', label: '标题', type: 'text', required: true },
      { key: 'descr', label: '说明', type: 'textarea' },
      { key: 'fieldCount', label: '字段数', type: 'number' },
      { key: 'submissions', label: '提交数', type: 'number' },
      { key: 'status', label: '状态', type: 'select', options: [
        { value: 'open', label: '开放' }, { value: 'published', label: '已发布' }, { value: 'closed', label: '关闭' }] },
    ],
  },
]
const RES_BY_KEY = Object.fromEntries(RES.map((r) => [r.key, r]))

// ── 通用表单弹层 ───────────────────────────────────────────
function RecordForm({ meta, initial, onClose, onSaved }: {
  meta: ResMeta; initial?: Rec; onClose: () => void; onSaved: () => void
}) {
  const [form, setForm] = useState<Rec>(() => {
    const f: Rec = {}
    for (const fd of meta.fields) {
      let v = initial?.[fd.key]
      if (fd.type === 'tags') v = Array.isArray(v) ? v.join(', ') : (v ?? '')
      else if (fd.type === 'bool') v = !!v
      else if (fd.type === 'number') v = v ?? ''
      else v = v ?? (fd.type === 'select' && fd.options ? fd.options[0].value : '')
      f[fd.key] = v
    }
    return f
  })
  const [busy, setBusy] = useState(false)

  const set = (k: string, v: any) => setForm((p) => ({ ...p, [k]: v }))

  const save = async () => {
    for (const fd of meta.fields) {
      if (fd.required && !String(form[fd.key] ?? '').trim()) return alert(`${fd.label}必填`)
    }
    setBusy(true)
    const payload: Rec = {}
    for (const fd of meta.fields) {
      let v: any = form[fd.key]
      if (fd.type === 'number') v = v === '' || v == null ? 0 : Number(v)
      else if (fd.type === 'tags') v = typeof v === 'string' ? v.split(',').map((s) => s.trim()).filter(Boolean) : (v || [])
      else if (fd.type === 'bool') v = !!v
      payload[fd.key] = v
    }
    const url = `/api/${meta.key}`
    const r = initial
      ? await mreq(`${url}/${initial.id}`, { method: 'PUT', body: JSON.stringify(payload) })
      : await mreq(url, { method: 'POST', body: JSON.stringify(payload) })
    setBusy(false)
    if (r.ok === false) return alert(r.error || '保存失败')
    onSaved()
  }

  return (
    <motion.div
      initial={{ opacity: 0, y: 28 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 28 }}
      transition={{ duration: 0.22, ease: [0.32, 0.72, 0, 1] }}
      className="fixed inset-0 z-30 bg-white flex flex-col"
    >
      <header className="sticky top-0 z-10 bg-white/90 backdrop-blur-xl border-b border-black/[0.04] px-5 py-4 flex items-center justify-between">
        <span className="text-[15px] font-semibold text-gray-900">{initial ? '编辑' : '新建'}{meta.label}</span>
        <button onClick={onClose} className="h-8 w-8 rounded-full bg-[#F1F2F7] text-gray-500 flex items-center justify-center active:scale-90 transition-transform">
          <X size={16} />
        </button>
      </header>
      <div className="flex-1 overflow-auto px-5 py-5 flex flex-col gap-4">
        {meta.fields.map((fd) => (
          <label key={fd.key} className="flex flex-col gap-1.5">
            <span className="text-[13px] font-medium text-gray-600">{fd.label}{fd.required ? ' *' : ''}</span>
            {fd.type === 'textarea' ? (
              <textarea value={form[fd.key] ?? ''} onChange={(e) => set(fd.key, e.target.value)}
                rows={3} className={`${INPUT} h-auto py-3`} />
            ) : fd.type === 'select' ? (
              <select value={form[fd.key] ?? ''} onChange={(e) => set(fd.key, e.target.value)} className={INPUT}>
                {fd.options!.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
              </select>
            ) : fd.type === 'number' ? (
              <input type="number" inputMode="decimal" value={form[fd.key] ?? ''} onChange={(e) => set(fd.key, e.target.value)} className={INPUT} />
            ) : (
              <input value={form[fd.key] ?? ''} onChange={(e) => set(fd.key, e.target.value)} className={INPUT} />
            )}
          </label>
        ))}
      </div>
      <div className="px-5 py-4 border-t border-black/[0.04] bg-white/90 backdrop-blur-xl">
        <button disabled={busy} onClick={() => void save()} className={BTN_PRIMARY}>
          {busy ? '保存中…' : '保存'}
        </button>
      </div>
    </motion.div>
  )
}

// ── 登录 / 改密 ────────────────────────────────────────────
function LoginScreen({ onOk }: { onOk: (token: string) => void }) {
  const [login, setLogin] = useState({ username: '', password: '' })
  const [needMcp, setNeedMcp] = useState(false)
  const [pw, setPw] = useState({ newPw: '', confirmPw: '' })
  const [busy, setBusy] = useState(false)

  const doLogin = async () => {
    setBusy(true)
    const r: any = await mreq('/api/auth/login', { method: 'POST', body: JSON.stringify(login) })
    setBusy(false)
    if (r.ok === false) return alert(r.error || '登录失败')
    localStorage.setItem('auth_token', r.data.token)
    if (r.data.user?.mustChangePassword) return setNeedMcp(true)
    onOk(r.data.token)
  }
  const doChangePw = async () => {
    if (pw.newPw.length < 8) return alert('新密码至少 8 位')
    if (pw.newPw !== pw.confirmPw) return alert('两次输入不一致')
    const r: any = await mreq('/api/me/password', {
      method: 'POST', body: JSON.stringify({ old_password: login.password, new_password: pw.newPw }),
    })
    if (r.ok === false) return alert(r.error || '修改失败')
    setNeedMcp(false)
    onOk(localStorage.getItem('auth_token') || '')
  }

  return (
    <div className="relative min-h-screen overflow-hidden bg-gradient-to-br from-[#5B3DF5] via-[#7C5CFF] to-[#A56BFF] flex flex-col justify-center px-6">
      {/* 氛围光斑 */}
      <div className="absolute -top-24 -right-24 h-72 w-72 rounded-full bg-white/10 blur-3xl" />
      <div className="absolute -bottom-32 -left-20 h-80 w-80 rounded-full bg-white/[0.07] blur-3xl" />
      <div className="absolute top-1/3 left-1/2 -translate-x-1/2 h-40 w-40 rounded-full bg-white/[0.06] blur-2xl" />

      <div className="relative w-full max-w-sm mx-auto flex flex-col gap-8">
        <div className="flex flex-col items-center gap-3.5">
          <motion.div
            initial={{ scale: 0.6, opacity: 0 }} animate={{ scale: 1, opacity: 1 }}
            transition={{ duration: 0.4, ease: 'backOut' }}
            className="h-16 w-16 rounded-[22px] bg-white/15 backdrop-blur border border-white/20 flex items-center justify-center shadow-xl shadow-black/10"
          >
            <Sparkles size={28} className="text-white" />
          </motion.div>
          <div className="text-center">
            <h1 className="text-[26px] font-bold text-white tracking-tight">AI 工作台</h1>
            <p className="text-white/70 text-sm mt-1">随时随地，掌上管理</p>
          </div>
        </div>

        <motion.div
          initial={{ y: 24, opacity: 0 }} animate={{ y: 0, opacity: 1 }}
          transition={{ duration: 0.35, delay: 0.08 }}
          className="rounded-[28px] bg-white p-6 shadow-2xl shadow-black/20 flex flex-col gap-3.5"
        >
          <div className="flex flex-col gap-1">
            <span className="text-[13px] font-medium text-gray-600">账号</span>
            <input value={login.username} onChange={(e) => setLogin({ ...login, username: e.target.value })} placeholder="用户名" autoCapitalize="none"
              className={INPUT} />
          </div>
          <div className="flex flex-col gap-1">
            <span className="text-[13px] font-medium text-gray-600">密码</span>
            <input value={login.password} onChange={(e) => setLogin({ ...login, password: e.target.value })} placeholder="密码" type="password"
              className={INPUT} />
          </div>
          <button onClick={() => void doLogin()} disabled={!login.username || !login.password || busy} className={`${BTN_PRIMARY} mt-1`}>
            {busy ? '登录中…' : '登录'}
          </button>

          {needMcp && (
            <motion.div
              initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: 'auto' }}
              className="mt-1 overflow-hidden rounded-2xl border border-amber-200 bg-amber-50 p-4 flex flex-col gap-2.5"
            >
              <p className="text-xs text-amber-700 font-medium">首次登录，请先设置新密码：</p>
              <input type="password" value={pw.newPw} onChange={(e) => setPw({ ...pw, newPw: e.target.value })} placeholder="新密码（至少 8 位）" className={`${INPUT} h-11 text-sm`} />
              <input type="password" value={pw.confirmPw} onChange={(e) => setPw({ ...pw, confirmPw: e.target.value })} placeholder="确认新密码" className={`${INPUT} h-11 text-sm`} />
              <button onClick={() => void doChangePw()} className="h-11 rounded-xl bg-gradient-to-r from-emerald-500 to-teal-500 text-white text-sm font-semibold shadow-lg shadow-emerald-500/25 active:scale-[0.98] transition-all">
                设置并继续
              </button>
            </motion.div>
          )}
        </motion.div>

        <p className="relative text-center text-[11px] text-white/50">登录即表示同意服务条款与隐私政策</p>
      </div>
    </div>
  )
}

// ── 待办审批 ──────────────────────────────────────────────
function ApprovalsTab({ role }: { role: string }) {
  const [tab, setTab] = useState<'pending' | 'done'>('pending')
  const [list, setList] = useState<Rec[] | null>(null)
  const [detail, setDetail] = useState<Rec | null>(null)
  const [busyId, setBusyId] = useState<string | null>(null)

  const load = useCallback(async () => {
    if (tab === 'pending') {
      const r: any = await mreq('/api/approvals?status=pending')
      setList(r.ok === false ? [] : (r.data ?? []))
    } else {
      const a: any = await mreq('/api/approvals?status=approved')
      const b: any = await mreq('/api/approvals?status=rejected')
      const arr = [...(a.ok === false ? [] : (a.data ?? [])), ...(b.ok === false ? [] : (b.data ?? []))]
      setList(arr)
    }
  }, [tab])

  useEffect(() => { void load() }, [load])
  useEffect(() => {
    if (tab !== 'pending') return
    const t = setInterval(() => void load(), 15_000)
    return () => clearInterval(t)
  }, [tab, load])

  const decide = async (id: string, status: 'approved' | 'rejected') => {
    setBusyId(id)
    const r: any = await mreq(`/api/approvals/${id}/decide`, {
      method: 'POST', body: JSON.stringify({ status }),
    })
    setBusyId(null)
    if (r.ok === false) return alert(r.error || '操作失败')
    setDetail(null)
    await load()
  }

  return (
    <div className="flex-1 flex flex-col min-h-0">
      {/* 分段切换 */}
      <div className="px-4 pt-4">
        <div className="p-1 bg-[#EDEEF5] rounded-full flex">
          {(['pending', 'done'] as const).map((t) => (
            <button key={t} onClick={() => setTab(t)}
              className={`flex-1 h-9 rounded-full text-[13px] font-medium transition-all ${tab === t ? 'bg-white text-[#6C4DF6] shadow-sm' : 'text-gray-500'}`}>
              {t === 'pending' ? '待我审批' : '已办'}
            </button>
          ))}
        </div>
      </div>

      <main className="flex-1 px-4 pt-4 pb-6 flex flex-col gap-3 overflow-auto">
        {list === null ? (
          <div className="flex flex-col items-center gap-3 py-20">
            <div className="h-9 w-9 rounded-full border-2 border-[#7C5CFF]/20 border-t-[#7C5CFF] animate-spin" />
            <p className="text-xs text-gray-400">加载中…</p>
          </div>
        ) : list.length === 0 ? (
          <div className="flex flex-col items-center gap-3 py-20 text-gray-300">
            <div className="h-16 w-16 rounded-3xl bg-white shadow-sm border border-black/[0.04] flex items-center justify-center">
              <Inbox size={28} className="text-gray-300" />
            </div>
            <p className="text-sm text-gray-400">{tab === 'pending' ? '没有待审批的操作' : '暂无已办记录'}</p>
          </div>
        ) : list.map((a) => (
          <motion.button
            key={a.id} layout initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }}
            onClick={() => setDetail(a)} className={`${CARD} p-4 text-left active:scale-[0.99] transition-transform`}
          >
            <div className="flex items-center gap-2 mb-2">
              <RiskBadge risk={a.risk} />
              <span className="text-[11px] px-2 py-0.5 rounded-lg bg-[#F1F2F7] text-gray-500 font-medium">
                {a.action === 'publish' ? '发布' : a.action === 'update' ? '更新' : a.action === 'delete' ? '删除' : a.action}
              </span>
              {a.status !== 'pending' && <StatusBadge s={a.status} />}
              <ChevronRight size={14} className="ml-auto text-gray-300" />
            </div>
            <p className="text-[15px] font-semibold text-gray-900 break-all">{a.target}</p>
            <p className="text-xs text-gray-500 mt-1 leading-relaxed">{a.summary}</p>
            <p className="text-[11px] text-gray-400 mt-2">发起：{a.requestedBy}</p>
          </motion.button>
        ))}
      </main>

      <AnimatePresence>
        {detail && (
          <motion.div
            initial={{ opacity: 0, y: 28 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 28 }}
            transition={{ duration: 0.22, ease: [0.32, 0.72, 0, 1] }}
            className="fixed inset-0 z-30 bg-white flex flex-col"
          >
            <header className="sticky top-0 z-10 bg-white/90 backdrop-blur-xl border-b border-black/[0.04] px-5 py-4 flex items-center justify-between">
              <span className="text-[15px] font-semibold text-gray-900">审批详情</span>
              <button onClick={() => setDetail(null)} className="h-8 w-8 rounded-full bg-[#F1F2F7] text-gray-500 flex items-center justify-center active:scale-90 transition-transform">
                <X size={16} />
              </button>
            </header>
            <div className="flex-1 overflow-auto px-5 py-5 flex flex-col gap-4">
              <div className={`${CARD} p-4 flex items-center gap-2`}>
                <RiskBadge risk={detail.risk} />
                <StatusBadge s={detail.status} />
              </div>
              <div className={`${CARD} p-4 flex flex-col divide-y divide-black/[0.04]`}>
                <Row k="操作" v={detail.action === 'publish' ? '发布' : detail.action === 'update' ? '更新' : detail.action === 'delete' ? '删除' : detail.action} />
                <Row k="对象" v={detail.target} />
                <Row k="说明" v={detail.summary} />
                <Row k="发起人" v={detail.requestedBy} />
                {detail.decidedAt && <Row k="裁决时间" v={detail.decidedAt} />}
              </div>
            </div>
            {detail.status === 'pending' && role === 'owner' && (
              <div className="px-5 py-4 border-t border-black/[0.04] bg-white/90 backdrop-blur-xl grid grid-cols-2 gap-2.5">
                <button disabled={busyId === detail.id} onClick={() => void decide(detail.id, 'approved')}
                  className="h-12 rounded-2xl bg-gradient-to-r from-emerald-500 to-teal-500 text-white font-semibold text-[15px] shadow-lg shadow-emerald-500/25 active:scale-[0.98] transition-all disabled:opacity-50">
                  <Check size={16} className="inline -mt-0.5 mr-1" />批准
                </button>
                <button disabled={busyId === detail.id} onClick={() => void decide(detail.id, 'rejected')}
                  className="h-12 rounded-2xl bg-white border border-red-200 text-red-500 font-semibold text-[15px] active:scale-[0.98] transition-all disabled:opacity-50">
                  ✕ 驳回
                </button>
              </div>
            )}
            {detail.status === 'pending' && role !== 'owner' && (
              <p className="p-4 text-center text-xs text-gray-400 border-t border-black/[0.04]">仅 Owner 可裁决</p>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}
function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex items-start justify-between gap-4 py-3">
      <span className="text-xs text-gray-400 shrink-0">{k}</span>
      <span className="text-sm text-gray-800 text-right break-all">{v || '—'}</span>
    </div>
  )
}

// ── 概览仪表盘 ────────────────────────────────────────────
function OverviewTab({ onGotoContent }: { onGotoContent: (key: string) => void }) {
  const [stats, setStats] = useState<Rec | null>(null)
  const CARDS: { key: string; label: string; field: string }[] = [
    { key: 'articles', label: '文章', field: 'articles' },
    { key: 'pages', label: '页面', field: 'pages' },
    { key: 'products', label: '产品', field: 'products' },
    { key: 'media', label: '媒体', field: 'media' },
    { key: 'customers', label: '客户', field: 'customers' },
    { key: 'leads', label: '线索', field: 'leads' },
    { key: 'forms', label: '表单', field: 'forms' },
  ]
  useEffect(() => {
    void (async () => {
      const keys = ['articles', 'pages', 'products', 'media', 'customers', 'leads', 'forms']
      const out: Rec = {}
      await Promise.all(keys.map(async (k) => {
        const r: any = await mreq(`/api/${k}`)
        out[k] = r.ok === false ? 0 : (r.total ?? (r.data?.length ?? 0))
      }))
      const ap: any = await mreq('/api/approvals?status=pending')
      out.pending = ap.ok === false ? 0 : (ap.total ?? ap.data?.length ?? 0)
      setStats(out)
    })()
  }, [])
  return (
    <main className="flex-1 px-4 pt-4 pb-6 flex flex-col gap-4 overflow-auto">
      {/* Hero 卡：待审批 */}
      <motion.button
        initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }}
        onClick={() => onGotoContent('approvals')}
        className="relative overflow-hidden rounded-3xl bg-gradient-to-br from-[#6C4DF6] via-[#7C5CFF] to-[#A06BFF] p-5 text-left text-white shadow-xl shadow-[#7C5CFF]/25 active:scale-[0.99] transition-transform"
      >
        <div className="absolute -right-10 -top-14 h-44 w-44 rounded-full bg-white/10" />
        <div className="absolute -right-2 -bottom-16 h-32 w-32 rounded-full bg-white/[0.08]" />
        <div className="relative flex items-center justify-between">
          <div className="flex flex-col gap-0.5">
            <div className="flex items-center gap-1.5 text-white/80 text-xs font-medium mb-1">
              <ClipboardCheck size={14} /> 待我审批
            </div>
            <span className="text-5xl font-bold leading-none tracking-tight">{stats ? stats.pending : '·'}</span>
            <span className="text-white/70 text-xs mt-2">点击查看待办详情</span>
          </div>
          <div className="h-14 w-14 rounded-2xl bg-white/15 backdrop-blur border border-white/10 flex items-center justify-center">
            <ClipboardCheck size={26} className="text-white" />
          </div>
        </div>
      </motion.button>

      {/* 统计卡 */}
      <div className="grid grid-cols-2 gap-3">
        {CARDS.map((c, i) => {
          const Icon = RES_ICON[c.key]
          const tone = RES_ICON_TONE[c.key]
          return (
            <motion.button
              key={c.key} layout initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.03 * i }}
              onClick={() => onGotoContent(c.key)}
              className={`${CARD} p-4 text-left active:scale-[0.98] transition-transform`}
            >
              <span className={`inline-flex h-9 w-9 rounded-xl bg-gradient-to-br ${tone} text-white items-center justify-center shadow-sm`}>
                <Icon size={17} strokeWidth={2.2} />
              </span>
              <p className="mt-3 text-[11px] text-gray-400">{c.label}</p>
              <p className="text-[22px] font-bold text-gray-900 leading-none mt-0.5 tracking-tight">{stats ? stats[c.field] : '·'}</p>
            </motion.button>
          )
        })}
      </div>

      <p className="text-center text-[11px] text-gray-300 pt-1">数据每次打开时刷新 · 点击卡片查看明细</p>
    </main>
  )
}

// ── 内容（全功能 CRUD）────────────────────────────────────
function ContentTab({ role, initialKey }: { role: string; initialKey?: string }) {
  const [key, setKey] = useState(initialKey && initialKey !== 'approvals' ? initialKey : 'articles')
  const [list, setList] = useState<Rec[] | null>(null)
  const [detail, setDetail] = useState<Rec | null>(null)
  const [editing, setEditing] = useState<Rec | null>(null)
  const [showForm, setShowForm] = useState(false)
  const meta = RES_BY_KEY[key]
  const canWrite = role !== 'viewer'

  const load = useCallback(async () => {
    const r: any = await mreq(`/api/${key}`)
    setList(r.ok === false ? [] : (r.data ?? []))
  }, [key])
  useEffect(() => { void load() }, [load])

  const remove = async (rec: Rec) => {
    if (!confirm(`确认删除「${meta.title(rec)}」？`)) return
    const r: any = await mreq(`/api/${key}/${rec.id}`, { method: 'DELETE' })
    if (r.ok === false) return alert(r.error || '删除失败')
    setDetail(null)
    await load()
  }

  const Icon = RES_ICON[key]
  const tone = RES_ICON_TONE[key]

  return (
    <div className="flex-1 flex flex-col min-h-0">
      {/* 资源切换 chips */}
      <div className="flex gap-2 overflow-x-auto px-4 py-3 bg-white/80 backdrop-blur-xl border-b border-black/[0.04] shrink-0">
        {RES.map((r) => {
          const C = RES_ICON[r.key]
          const active = key === r.key
          return (
            <button key={r.key} onClick={() => setKey(r.key)}
              className={`flex items-center gap-1.5 whitespace-nowrap px-3.5 h-9 rounded-full text-[13px] font-medium transition-all active:scale-95 ${active ? `bg-gradient-to-r ${BRAND} text-white shadow-md shadow-[#7C5CFF]/25` : 'bg-[#F1F2F7] text-gray-500'}`}>
              <C size={14} strokeWidth={2.2} />{r.label}
            </button>
          )
        })}
      </div>

      <main className="flex-1 px-4 pt-4 pb-6 flex flex-col gap-2.5 overflow-auto">
        {list === null ? (
          <div className="flex flex-col items-center gap-3 py-20">
            <div className="h-9 w-9 rounded-full border-2 border-[#7C5CFF]/20 border-t-[#7C5CFF] animate-spin" />
            <p className="text-xs text-gray-400">加载中…</p>
          </div>
        ) : list.length === 0 ? (
          <div className="flex flex-col items-center gap-3 py-20 text-gray-300">
            <div className={`h-16 w-16 rounded-3xl bg-gradient-to-br ${tone} bg-opacity-10 flex items-center justify-center`}>
              <Icon size={26} className="text-white" />
            </div>
            <p className="text-sm text-gray-400">暂无{meta.label}数据</p>
          </div>
        ) : list.map((rec, i) => (
          <motion.button
            key={rec.id} layout initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.02 * i }}
            onClick={() => setDetail(rec)}
            className={`${CARD} p-3.5 text-left active:scale-[0.99] transition-transform`}
          >
            <div className="flex items-center gap-3 min-w-0">
              <span className={`h-10 w-10 rounded-2xl bg-gradient-to-br ${tone} text-white flex items-center justify-center shrink-0 shadow-sm`}>
                <Icon size={17} strokeWidth={2.2} />
              </span>
              <div className="min-w-0 flex-1">
                <p className="text-[15px] font-semibold text-gray-900 truncate">{meta.title(rec)}</p>
                {meta.subtitle && meta.subtitle(rec) && (
                  <p className="text-xs text-gray-400 truncate mt-0.5">{meta.subtitle(rec)}</p>
                )}
              </div>
              <div className="flex items-center gap-1.5 shrink-0">
                {meta.statusField && <StatusBadge s={rec[meta.statusField]} />}
                <ChevronRight size={15} className="text-gray-300" />
              </div>
            </div>
          </motion.button>
        ))}
      </main>

      {canWrite && (
        <button onClick={() => { setEditing(null); setShowForm(true) }}
          className="fixed right-5 bottom-24 z-20 h-14 w-14 rounded-2xl bg-gradient-to-br from-[#6C4DF6] to-[#A06BFF] text-white shadow-lg shadow-[#7C5CFF]/35 flex items-center justify-center active:scale-90 transition-transform">
          <Plus size={26} strokeWidth={2.5} />
        </button>
      )}

      <AnimatePresence>
        {detail && (
          <motion.div
            initial={{ opacity: 0, y: 28 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 28 }}
            transition={{ duration: 0.22, ease: [0.32, 0.72, 0, 1] }}
            className="fixed inset-0 z-30 bg-white flex flex-col"
          >
            <header className="sticky top-0 z-10 bg-white/90 backdrop-blur-xl border-b border-black/[0.04] px-5 py-4 flex items-center justify-between">
              <span className="text-[15px] font-semibold text-gray-900">{meta.label}详情</span>
              <button onClick={() => setDetail(null)} className="h-8 w-8 rounded-full bg-[#F1F2F7] text-gray-500 flex items-center justify-center active:scale-90 transition-transform">
                <X size={16} />
              </button>
            </header>
            <div className="flex-1 overflow-auto px-5 py-5 flex flex-col gap-4">
              <div className={`${CARD} p-4 flex flex-col divide-y divide-black/[0.04]`}>
                {meta.fields.map((fd) => {
                  let v: any = detail[fd.key]
                  if (fd.type === 'tags') v = Array.isArray(v) ? v.join(', ') : (v ?? '')
                  if (fd.type === 'select') v = STATUS_LABEL[v] ?? v
                  if (fd.type === 'bool') v = v ? '是' : '否'
                  return <Row key={fd.key} k={fd.label} v={v == null ? '' : String(v)} />
                })}
              </div>
            </div>
            {canWrite && (
              <div className="px-5 py-4 border-t border-black/[0.04] bg-white/90 backdrop-blur-xl grid grid-cols-2 gap-2.5">
                <button onClick={() => { setEditing(detail); setShowForm(true); setDetail(null) }}
                  className={`h-12 rounded-2xl bg-gradient-to-r ${BRAND} text-white font-semibold text-[15px] shadow-lg shadow-[#7C5CFF]/25 active:scale-[0.98] transition-all`}>
                  <Pencil size={14} className="inline -mt-0.5 mr-1" />编辑
                </button>
                <button onClick={() => void remove(detail)}
                  className="h-12 rounded-2xl bg-white border border-red-200 text-red-500 font-semibold text-[15px] active:scale-[0.98] transition-all">
                  <Trash2 size={14} className="inline -mt-0.5 mr-1" />删除
                </button>
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {showForm && (
          <RecordForm meta={meta} initial={editing ?? undefined}
            onClose={() => setShowForm(false)} onSaved={() => { setShowForm(false); void load() }} />
        )}
      </AnimatePresence>
    </div>
  )
}

// ── 我的 ──────────────────────────────────────────────────
function ProfileTab({ token, role, onLogout }: { token: string; role: string; onLogout: () => void }) {
  const [me, setMe] = useState<Rec | null>(null)
  const [pw, setPw] = useState({ oldPw: '', newPw: '', confirmPw: '' })
  const [editing, setEditing] = useState(false)

  useEffect(() => {
    void (async () => {
      const r: any = await mreq('/api/user/me')
      if (r.ok !== false) setMe(r.data ?? r)
    })()
  }, [token])

  const doChangePw = async () => {
    if (!pw.oldPw) return alert('请输入旧密码')
    if (pw.newPw.length < 8) return alert('新密码至少 8 位')
    if (pw.newPw !== pw.confirmPw) return alert('两次输入不一致')
    if (pw.newPw === pw.oldPw) return alert('新密码不能与旧密码相同')
    const r: any = await mreq('/api/me/password', {
      method: 'POST', body: JSON.stringify({ old_password: pw.oldPw, new_password: pw.newPw }),
    })
    if (r.ok === false) return alert(r.error || '修改失败')
    alert('密码已更新'); setEditing(false); setPw({ oldPw: '', newPw: '', confirmPw: '' })
  }

  const roleLabel = role === 'owner' ? '管理员' : role === 'editor' ? '编辑' : '访客'
  const name = me?.nickname || me?.username || '加载中…'

  return (
    <main className="flex-1 px-4 pt-4 pb-6 flex flex-col gap-3 overflow-auto">
      {/* 用户卡 */}
      <motion.div initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }}
        className={`${CARD} p-5 flex items-center gap-3.5`}>
        <div className="h-14 w-14 rounded-2xl bg-gradient-to-br from-[#6C4DF6] to-[#A06BFF] text-white flex items-center justify-center text-xl font-bold shadow-md shadow-[#7C5CFF]/25 shrink-0">
          {(me?.nickname || me?.username || '?').slice(0, 1)}
        </div>
        <div className="min-w-0">
          <p className="text-base font-semibold text-gray-900 truncate">{name}</p>
          <p className="text-xs text-gray-400">@{me?.username}</p>
        </div>
        <span className="ml-auto px-2.5 py-1 rounded-full bg-[#7C5CFF]/10 text-[#6C4DF6] text-[11px] font-semibold">
          {roleLabel}
        </span>
      </motion.div>

      {/* 信息卡 */}
      <div className={`${CARD} px-5 py-2 flex flex-col divide-y divide-black/[0.04]`}>
        <Row k="角色" v={role} />
        <Row k="权限数" v={String((me?.permissions?.length) ?? 0)} />
        {me?.email && <Row k="邮箱" v={me.email} />}
      </div>

      {!editing ? (
        <button onClick={() => setEditing(true)}
          className={`${CARD} py-4 flex items-center justify-center gap-2 text-gray-600 font-medium text-[15px] active:scale-[0.99] transition-transform`}>
          <KeyRound size={16} className="text-gray-400" />修改密码
        </button>
      ) : (
        <motion.div initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} className={`${CARD} p-4 flex flex-col gap-2.5`}>
          <input type="password" value={pw.oldPw} onChange={(e) => setPw({ ...pw, oldPw: e.target.value })} placeholder="旧密码" className={INPUT} />
          <input type="password" value={pw.newPw} onChange={(e) => setPw({ ...pw, newPw: e.target.value })} placeholder="新密码（至少 8 位）" className={INPUT} />
          <input type="password" value={pw.confirmPw} onChange={(e) => setPw({ ...pw, confirmPw: e.target.value })} placeholder="确认新密码" className={INPUT} />
          <div className="grid grid-cols-2 gap-2.5 mt-1">
            <button onClick={() => setEditing(false)} className="h-11 rounded-2xl bg-white border border-black/[0.08] text-gray-600 font-medium text-[15px] active:scale-[0.98] transition-transform">取消</button>
            <button onClick={() => void doChangePw()} className="h-11 rounded-2xl bg-gradient-to-r from-emerald-500 to-teal-500 text-white font-semibold text-[15px] shadow-lg shadow-emerald-500/25 active:scale-[0.98] transition-all">保存</button>
          </div>
        </motion.div>
      )}

      <button onClick={onLogout}
        className={`${CARD} py-4 flex items-center justify-center gap-2 text-red-500 font-medium text-[15px] active:scale-[0.99] transition-transform`}>
        <LogOut size={16} />退出登录
      </button>
    </main>
  )
}

// ── 根组件 ────────────────────────────────────────────────
function MobileWorkbench() {
  const [token, setToken] = useState<string | null>(() => localStorage.getItem('auth_token'))
  const [role, setRole] = useState<string>('viewer')
  const [tab, setTab] = useState<'approvals' | 'overview' | 'content' | 'me'>('approvals')
  const [pendingCount, setPendingCount] = useState(0)
  const [contentKey, setContentKey] = useState<string | undefined>(undefined)

  useEffect(() => {
    if (!token) return
    void (async () => {
      const r: any = await mreq('/api/user/me')
      if (r.ok !== false) { setRole(r.data?.role ?? r.role ?? 'viewer') }
      const ap: any = await mreq('/api/approvals?status=pending')
      setPendingCount(ap.ok === false ? 0 : (ap.total ?? ap.data?.length ?? 0))
    })()
  }, [token])

  useEffect(() => {
    document.title = pendingCount > 0 ? `（${pendingCount}）待审批 · AI 工作台` : 'AI 工作台'
  }, [pendingCount])

  if (!token) return <LoginScreen onOk={(t) => setToken(t)} />

  const TABS: { key: 'approvals' | 'overview' | 'content' | 'me'; label: string; icon: LucideIcon }[] = [
    { key: 'approvals', label: '待办', icon: ClipboardCheck },
    { key: 'overview', label: '概览', icon: LayoutDashboard },
    { key: 'content', label: '内容', icon: FolderOpen },
    { key: 'me', label: '我的', icon: User },
  ]

  const gotoContent = (k: string) => {
    if (k === 'approvals') { setTab('approvals'); return }
    setContentKey(k); setTab('content')
  }

  return (
    <div className="min-h-screen bg-[#F4F5FA] flex flex-col">
      {/* 渐变品牌 Header */}
      <header className="relative shrink-0 bg-gradient-to-r from-[#5B3DF5] via-[#7C5CFF] to-[#A56BFF] text-white overflow-hidden">
        <div className="absolute -right-10 -top-16 h-40 w-40 rounded-full bg-white/10" />
        <div className="absolute right-16 -bottom-12 h-24 w-24 rounded-full bg-white/[0.07]" />
        <div className="relative flex items-center justify-between px-5 pt-[calc(env(safe-area-inset-top,0px)+14px)] pb-4">
          <div className="flex items-center gap-2.5">
            <div className="h-9 w-9 rounded-[12px] bg-white/20 backdrop-blur border border-white/15 flex items-center justify-center shadow-sm">
              <Sparkles size={17} strokeWidth={2.2} />
            </div>
            <div>
              <h1 className="text-[15px] font-bold leading-tight tracking-tight">AI 工作台</h1>
              <p className="text-[10px] text-white/70 leading-tight">移动管理中心</p>
            </div>
          </div>
          <button
            onClick={() => { localStorage.removeItem('auth_token'); setToken(null) }}
            className="flex items-center gap-1.5 rounded-full bg-white/15 backdrop-blur border border-white/15 px-3 py-1.5 text-xs font-medium active:scale-95 transition-transform">
            <LogOut size={13} />退出
          </button>
        </div>
      </header>

      {/* Tab 内容（带切换动画） */}
      <div className="flex-1 flex flex-col min-h-0">
        <AnimatePresence mode="wait">
          <motion.div
            key={tab}
            initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -10 }}
            transition={{ duration: 0.18, ease: 'easeOut' }}
            className="flex-1 flex flex-col min-h-0"
          >
            {tab === 'approvals' && <ApprovalsTab role={role} />}
            {tab === 'overview' && <OverviewTab onGotoContent={gotoContent} />}
            {tab === 'content' && <ContentTab role={role} initialKey={contentKey} />}
            {tab === 'me' && <ProfileTab token={token} role={role} onLogout={() => { localStorage.removeItem('auth_token'); setToken(null) }} />}
          </motion.div>
        </AnimatePresence>
      </div>

      {/* 底部 TabBar */}
      <nav className="sticky bottom-0 z-20 bg-white/90 backdrop-blur-xl border-t border-black/[0.04] grid grid-cols-4 shrink-0 pb-[env(safe-area-inset-bottom,0px)]">
        {TABS.map((t) => {
          const active = tab === t.key
          const Icon = t.icon
          return (
            <button key={t.key} onClick={() => setTab(t.key)}
              className="relative flex flex-col items-center gap-1 py-2.5 active:scale-95 transition-transform">
              <span className={`relative flex h-7 w-14 items-center justify-center rounded-full transition-colors duration-200 ${active ? 'bg-[#7C5CFF]/10' : ''}`}>
                <Icon size={21} strokeWidth={active ? 2.4 : 1.8} className={active ? 'text-[#6C4DF6]' : 'text-gray-400'} />
                {t.key === 'approvals' && pendingCount > 0 && (
                  <span className="absolute -top-1 right-2.5 min-w-[17px] h-[17px] px-1 rounded-full bg-red-500 text-white text-[10px] font-bold flex items-center justify-center shadow-sm ring-2 ring-white">
                    {pendingCount > 99 ? '99+' : pendingCount}
                  </span>
                )}
              </span>
              <span className={`text-[10.5px] leading-none font-medium ${active ? 'text-[#6C4DF6]' : 'text-gray-400'}`}>{t.label}</span>
            </button>
          )
        })}
      </nav>
    </div>
  )
}

export const Route = createFileRoute('/m')({
  component: MobileWorkbench,
})
