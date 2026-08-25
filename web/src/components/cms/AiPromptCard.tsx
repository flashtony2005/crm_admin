import { useState } from 'react'
import { useNavigate } from '@tanstack/react-router'

const SUGGESTIONS = [
  '帮我看看今天有没有重要客户',
  '把昨天的新品发布掉，并写一篇介绍文章',
  '给本周新客户生成一条问候回复',
]

/**
 * AI 指令输入卡片（抽象组件）：Home 快捷入口与 Assistant 页共用。
 * 提交后携带 q 参数跳转到 AI Assistant。
 */
export function AiPromptCard({ compact = false }: { compact?: boolean }) {
  const navigate = useNavigate()
  const [value, setValue] = useState('')

  const run = () => {
    const q = value.trim()
    if (!q) return
    navigate({ to: '/ai/assistant', search: { q } })
    setValue('')
  }

  return (
    <div className="rounded-xl border border-os-border bg-white p-4 md:p-5 shadow-sm">
      <div className="flex items-center gap-2 mb-3">
        <span className="w-7 h-7 rounded-lg bg-gradient-to-br from-indigo-500 to-violet-500 text-white flex items-center justify-center text-sm">
          ✦
        </span>
        <span className="text-sm font-medium text-os-text-primary">让 AI 帮你完成工作</span>
      </div>

      <div className="flex gap-2">
        <input
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && run()}
          placeholder={compact ? '输入指令…' : '例如：把新品 X 发布到网站，并写一篇介绍文章。'}
          aria-label="向 AI 描述你想完成的工作"
          className="flex-1 h-10 px-3.5 text-sm rounded-lg border border-os-border outline-none focus:border-[#6366f1] focus:ring-2 focus:ring-indigo-100 transition-all"
        />
        <button
          type="button"
          onClick={run}
          disabled={!value.trim()}
          className="h-10 px-4 rounded-lg bg-[#6366F1] text-white text-sm font-medium hover:bg-[#4F46E5] disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
        >
          执行
        </button>
      </div>

      {!compact && (
        <div className="flex flex-wrap gap-2 mt-3">
          {SUGGESTIONS.map((s) => (
            <button
              key={s}
              type="button"
              onClick={() => setValue(s)}
              className="text-xs px-2.5 py-1 rounded-full bg-gray-100 text-gray-600 hover:bg-indigo-50 hover:text-indigo-600 transition-colors"
            >
              {s}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
