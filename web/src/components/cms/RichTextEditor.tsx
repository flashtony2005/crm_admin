import { useEffect, useRef, useState } from 'react'
import { htmlToMarkdown, markdownToHtml } from './markdown'
import { uploadFile } from '../../api/cms'

interface Props {
  value: string
  onChange: (html: string) => void
  placeholder?: string
  height?: number
}

/**
 * 零依赖富文本编辑器（contentEditable + execCommand），支持双模式：
 * - 富文本（所见即所得）：工具栏 加粗 / 斜体 / 下划线 / 标题 / 正文 / 列表 / 链接 / 图片
 * - Markdown 源码：写作者偏好，实时转换为 HTML（后端 content 仍存 HTML）
 * - 图片：以 FileReader 读为 data URL 后内联插入 <img>（内联 base64 方案，
 *   纯前端、无需后端上传端点）；Markdown 模式下可直接写 ![alt](url)
 * 注：execCommand 虽被标记 deprecated，但在所有现代浏览器中仍可用，
 * 作为轻量 CMS 编辑器足够；后续可平滑替换为 TipTap。
 */
export function RichTextEditor({ value, onChange, placeholder, height = 320 }: Props) {
  const ref = useRef<HTMLDivElement>(null)
  const fileRef = useRef<HTMLInputElement>(null)
  const [mode, setMode] = useState<'richtext' | 'markdown'>('richtext')
  const [mdText, setMdText] = useState('')

  // 外部值变化（编辑回填 / 切换记录）时同步到 DOM；
  // 仅在内容不一致时写入，避免正在输入时光标跳动。
  useEffect(() => {
    const el = ref.current
    if (mode === 'richtext' && el && value !== el.innerHTML) {
      el.innerHTML = value || ''
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value])

  const emit = () => {
    if (ref.current) onChange(ref.current.innerHTML)
  }

  const exec = (cmd: string, arg?: string) => {
    ref.current?.focus()
    document.execCommand(cmd, false, arg)
    emit()
  }

  const onBlock = (tag: string) => {
    ref.current?.focus()
    document.execCommand('formatBlock', false, tag)
    emit()
  }

  const onLink = () => {
    const url = window.prompt('输入链接地址（例如 https://example.com）', 'https://')
    if (!url) return
    ref.current?.focus()
    document.execCommand('createLink', false, url)
    emit()
  }

  // 模式切换：富文本 ⇄ Markdown（双向转换保留内容）
  const switchMode = () => {
    if (mode === 'richtext') {
      setMdText(htmlToMarkdown(ref.current?.innerHTML ?? ''))
      setMode('markdown')
    } else {
      const html = markdownToHtml(mdText)
      if (ref.current) ref.current.innerHTML = html
      setMode('richtext')
      onChange(html)
    }
  }

  const onMdChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setMdText(e.target.value)
    onChange(markdownToHtml(e.target.value))
  }

  const onPickImage = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    e.target.value = ''
    if (!file) return
    if (!file.type.startsWith('image/')) {
      window.alert('请选择图片文件')
      return
    }
    try {
      const uploaded = await uploadFile(file)
      ref.current?.focus()
      document.execCommand(
        'insertHTML',
        false,
        `<img src="${uploaded.large ?? uploaded.url}" alt="${file.name}" style="max-width:100%;height:auto;border-radius:8px;margin:8px 0;display:block;" />`,
      )
      emit()
    } catch {
      window.alert('图片上传失败，请重试')
    }
  }

  const btn =
    'px-2.5 h-8 rounded-md text-sm text-gray-600 hover:bg-indigo-50 hover:text-indigo-600 transition-colors select-none'

  return (
    <div className="border border-gray-200 rounded-lg overflow-hidden bg-white">
      <style>{`.rte-content:empty::before{content:attr(data-placeholder);color:#9ca3af;}`}</style>
      <div className="flex flex-wrap items-center gap-1 px-2 py-1.5 border-b border-gray-100 bg-gray-50">
        {mode === 'richtext' ? (
          <>
            <button type="button" title="加粗" onClick={() => exec('bold')} className={`${btn} font-bold`}>B</button>
            <button type="button" title="斜体" onClick={() => exec('italic')} className={`${btn} italic`}>I</button>
            <button type="button" title="下划线" onClick={() => exec('underline')} className={`${btn} underline`}>U</button>
            <span className="w-px h-5 bg-gray-200 mx-1" />
            <button type="button" title="标题" onClick={() => onBlock('H2')} className={`${btn} font-semibold`}>H2</button>
            <button type="button" title="正文段落" onClick={() => onBlock('P')} className={btn}>¶</button>
            <span className="w-px h-5 bg-gray-200 mx-1" />
            <button type="button" title="无序列表" onClick={() => exec('insertUnorderedList')} className={btn}>• 列表</button>
            <button type="button" title="有序列表" onClick={() => exec('insertOrderedList')} className={btn}>1. 列表</button>
            <span className="w-px h-5 bg-gray-200 mx-1" />
            <button type="button" title="插入链接" onClick={onLink} className={btn}>🔗 链接</button>
            <button type="button" title="插入图片" onClick={() => fileRef.current?.click()} className={btn}>🖼 图片</button>
            <input ref={fileRef} type="file" accept="image/*" className="hidden" onChange={onPickImage} />
          </>
        ) : (
          <span className="px-2 text-xs text-gray-400">
            Markdown 源码模式 · 支持 # 标题 / **粗体** / - 列表 / [链接](url) / ![图片](url) / ``` 代码块
          </span>
        )}
        <span className="flex-1" />
        <button
          type="button"
          onClick={switchMode}
          title={mode === 'richtext' ? '切换到 Markdown 源码' : '切回所见即所得'}
          className={`px-2.5 h-7 rounded-md text-xs font-medium transition-colors select-none ${
            mode === 'markdown'
              ? 'bg-indigo-600 text-white'
              : 'bg-indigo-50 text-indigo-600 hover:bg-indigo-100'
          }`}
        >
          {mode === 'richtext' ? 'MD 源码' : '可视化'}
        </button>
      </div>
      {mode === 'richtext' ? (
        <div
          ref={ref}
          contentEditable
          suppressContentEditableWarning
          onInput={emit}
          data-placeholder={placeholder}
          style={{ minHeight: height }}
          className="rte-content px-3 py-2 text-sm leading-relaxed outline-none focus:ring-2 focus:ring-indigo-100 overflow-auto max-h-[60vh]"
        />
      ) : (
        <textarea
          value={mdText}
          onChange={onMdChange}
          placeholder={placeholder ? `${placeholder}（Markdown）` : '用 Markdown 撰写…'}
          style={{ minHeight: height }}
          className="w-full px-3 py-2 text-sm leading-relaxed font-mono outline-none focus:ring-2 focus:ring-indigo-100 resize-y overflow-auto"
        />
      )}
    </div>
  )
}
