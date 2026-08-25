import { useEffect, useRef, useState } from 'react'
import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'

import { AiPromptCard } from '../../components/cms/AiPromptCard'
import { StatusBadge } from '../../components/common/StatusBadge'
import { articlesApi, CMS_MODE } from '../../api/cms'
import { request } from '../../api/client'
import { extractError } from '../../lib/error'

/** URL 查询参数：?q=用户指令（Header 全局 AI 输入与 Home 卡片都跳到这里） */
type AssistantSearch = { q?: string }
export const Route = createFileRoute('/ai/assistant')({
  validateSearch: (s: Record<string, unknown>): AssistantSearch => ({
    q: typeof s.q === 'string' && s.q.trim() ? s.q.trim() : undefined,
  }),
  component: AssistantPage,
})

interface ChatMessage {
  id: number
  role: 'user' | 'ai'
  text?: string
  /** AI 消息可附带执行步骤（理解 → 操作 → 审批 → 完成） */
  steps?: { label: string; status: 'done' | 'running' | 'pending' }[]
}

let nextId = 1

function buildPlanReply(prompt: string): ChatMessage {
  return {
    id: nextId++,
    role: 'ai',
    text: `好的，我来处理「${prompt}」。这是我的执行计划：`,
    steps: [
      { label: '搜索并读取相关内容', status: 'done' },
      { label: '生成 / 修改内容草稿', status: 'done' },
      { label: 'SEO 与风险检查通过', status: 'done' },
      { label: '已创建发布审批请求，等待你的批准', status: 'running' },
      { label: '执行发布', status: 'pending' },
    ],
  }
}

function aiText(text: string): ChatMessage {
  return { id: nextId++, role: 'ai', text }
}

/** 真实模式：调用后端 AI 执行器（Capability → Policy → Action → Audit） */
async function realInvoke(prompt: string): Promise<ChatMessage> {
  const trimmed = prompt.trim()
  try {
    // 「发布…」→ 发布最新草稿（缺发布权时后端转审批，这正是 G4 验收点）
    if (/^发布/.test(trimmed)) {
      const arts = await articlesApi.list()
      const draft = arts.find((a) => a.status === 'draft')
      if (!draft) return aiText('当前没有草稿文章可发布。先让我写一篇，或在 内容 → Articles 新建。')
      const body = await request<{ ok: boolean; data: { decision: string; message?: string } }>('/api/ai/invoke', {
        method: 'POST',
        body: JSON.stringify({ capability: 'content.articles.publish', input: { article_id: draft.id } }),
      })
      const d = body.data
      if (d.decision === 'executed') return aiText(`「${draft.title}」已直接发布 ✅`)
      return aiText(d.message ?? '发布请求已提交，等待 Owner 批准。可在 AI → Approvals 中查看。')
    }
    // 其余指令 → 写草稿
    const body = await request<{ ok: boolean; data: { result: { title: string } } }>('/api/ai/invoke', {
      method: 'POST',
      body: JSON.stringify({ capability: 'content.articles.draft', input: { topic: trimmed } }),
    })
    return aiText(`已生成草稿《${body.data.result.title}》，在 内容 → Articles 中查看（草稿状态，发布需审批）。`)
  } catch (e) {
    return aiText(`执行失败：${extractError(e, '请稍后再试')}`)
  }
}

const WELCOME: ChatMessage = {
  id: 0,
  role: 'ai',
  text: '你好！我是你的经营助手。可以直接告诉我要做什么，例如：「把新品 X 发布到网站，并写一篇介绍文章。」重要操作我会先请求你的批准，不会擅自发布。',
}

function AssistantPage() {
  const { q } = Route.useSearch()
  const navigate = useNavigate()
  const [messages, setMessages] = useState<ChatMessage[]>([WELCOME])
  const [thinking, setThinking] = useState(false)
  const bottomRef = useRef<HTMLDivElement>(null)

  // 携带 ?q= 进入时自动执行一轮演示对话
  useEffect(() => {
    if (!q) return
    setMessages((prev) => [...prev, { id: nextId++, role: 'user', text: q }])
    setThinking(true)
    if (CMS_MODE === 'real') {
      realInvoke(q)
        .then((msg) => {
          setMessages((prev) => [...prev, msg])
          setThinking(false)
          navigate({ to: '/ai/assistant', search: {}, replace: true })
        })
        .catch(() => setThinking(false))
      return () => {}
    }
    const timer = setTimeout(() => {
      setMessages((prev) => [...prev, buildPlanReply(q)])
      setThinking(false)
      // 清掉 q，刷新/回退不会重复触发
      navigate({ to: '/ai/assistant', search: {}, replace: true })
    }, 900)
    return () => clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [q])

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages, thinking])


  return (
    <div className="p-1 md:p-2 flex flex-col h-full gap-4">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-os-text-primary tracking-tight">AI 助手</h1>
          <p className="text-sm text-os-text-muted mt-0.5">
            理解 → 操作 → 审批 → 完成；AI 是执行者，但发布权在你手里。
          </p>
        </div>
        <Link
          to="/ai/approvals"
          className="text-xs px-3 py-1.5 rounded-lg bg-[#FFF7ED] text-orange-600 no-underline hover:bg-orange-100 transition-colors"
        >
          待审批 →
        </Link>
      </div>

      {/* 对话区 */}
      <div className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-4 pr-1">
        {messages.map((m) =>
          m.role === 'user' ? (
            <div key={m.id} className="self-end max-w-[80%] rounded-xl rounded-br-sm bg-[#6366F1] text-white px-4 py-2.5 text-sm shadow-sm">
              {m.text}
            </div>
          ) : (
            <div key={m.id} className="self-start max-w-[85%] rounded-xl border border-os-border bg-white px-4 py-3 shadow-sm">
              <div className="flex items-center gap-1.5 mb-1.5">
                <span className="w-5 h-5 rounded-md bg-gradient-to-br from-indigo-500 to-violet-500 text-white flex items-center justify-center text-[10px]">✦</span>
                <span className="text-xs font-medium text-os-text-secondary">AI 助手</span>
              </div>
              <p className="text-sm text-os-text-primary leading-relaxed">{m.text}</p>
              {m.steps && (
                <ol className="mt-3 flex flex-col gap-1.5 border-t border-os-border-light pt-3">
                  {m.steps.map((s, i) => (
                    <li key={i} className="flex items-center gap-2 text-sm">
                      <StatusBadge tone={s.status === 'done' ? 'success' : s.status === 'running' ? 'info' : 'neutral'}>
                        {s.status === 'done' ? '完成' : s.status === 'running' ? '进行中' : '待执行'}
                      </StatusBadge>
                      <span className={s.status === 'pending' ? 'text-os-text-muted' : 'text-os-text-primary'}>{s.label}</span>
                    </li>
                  ))}
                </ol>
              )}
            </div>
          ),
        )}
        {thinking && (
          <div className="self-start rounded-xl border border-os-border bg-white px-4 py-3 text-sm text-os-text-muted shadow-sm">
            ✦ 正在思考…
          </div>
        )}
        <div ref={bottomRef} />
      </div>

      {/* 底部输入 */}
      <AiPromptCard compact />
    </div>
  )
}
