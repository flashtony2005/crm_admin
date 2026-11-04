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
    <div className="award-ai-card rounded-xl border border-os-border bg-white p-4 md:p-5 shadow-sm">
      <div className="flex items-center gap-2 mb-3">
        <span className="award-ai-mark">
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
          className="award-ai-input flex-1 h-10 px-3.5 text-sm rounded-lg border border-os-border outline-none transition-all"
        />
        <button
          type="button"
          onClick={run}
          disabled={!value.trim()}
          className="award-ai-run h-10 px-4 rounded-lg text-white text-sm font-medium disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
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
              className="award-suggestion text-xs px-2.5 py-1 transition-colors"
            >
              {s}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
