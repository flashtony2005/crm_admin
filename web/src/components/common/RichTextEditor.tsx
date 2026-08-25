// @refresh reset
import { useState, useCallback, useRef, useEffect } from 'react'

interface Props {
  value: string
  onChange: (html: string) => void
  placeholder?: string
}

/**
 * 轻量富文本编辑器（零外部依赖）
 *
 * 使用 contentEditable + document.execCommand，所有现代浏览器原生支持。
 * 存储格式：HTML（与后端 TEXT 列兼容）。
 */
export default function RichTextEditor({ value, onChange, placeholder }: Props) {
  const editorRef = useRef<HTMLDivElement>(null)
  const [showLinkInput, setShowLinkInput] = useState(false)
  const [linkUrl, setLinkUrl] = useState('')

  // 初始化内容
  useEffect(() => {
    if (editorRef.current && !editorRef.current.innerHTML) {
      editorRef.current.innerHTML = value || ''
    }
  }, [])

  const exec = useCallback((cmd: string, val?: string) => {
    document.execCommand(cmd, false, val)
    editorRef.current?.focus()
    emitChange()
  }, [])

  const emitChange = useCallback(() => {
    const html = editorRef.current?.innerHTML || ''
    onChange(html === '<br>' ? '' : html)
  }, [onChange])

  const handlePaste = useCallback((e: React.ClipboardEvent) => {
    e.preventDefault()
    const text = e.clipboardData.getData('text/plain')
    document.execCommand('insertText', false, text)
    emitChange()
  }, [emitChange])

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Tab') {
      e.preventDefault()
      document.execCommand('insertHTML', false, '&emsp;')
    }
  }, [])

  const setLink = useCallback(() => {
    const url = linkUrl.trim()
    if (!url) return
    const selection = window.getSelection()
    if (selection && selection.toString()) {
      // 选中文本 → 插入链接
      document.execCommand('createLink', false, url)
    } else {
      // 无选中 → 插入自定义文本链接
      const html = `<a href="${url}" target="_blank" class="underline decoration-dotted underline-offset-2 text-primary-600 hover:decoration-solid">${url}</a>`
      document.execCommand('insertHTML', false, html)
    }
    setLinkUrl('')
    setShowLinkInput(false)
    emitChange()
  }, [linkUrl, emitChange])

  const btn = 'inline-flex items-center justify-center w-7 h-7 text-xs rounded transition-colors cursor-pointer hover:bg-os-bg-hover disabled:opacity-30 disabled:cursor-not-allowed'

  const is = (cmd: string, _val?: string) => {
    try { return document.queryCommandState(cmd) } catch { return false }
  }

  const isLinkActive = () => {
    try { return document.queryCommandEnabled('createLink') && !!document.queryCommandValue('createLink') } catch { return false }
  }

  return (
    <div className="border border-os-border dark:border-os-border rounded-os-lg overflow-hidden bg-white dark:bg-os-bg-card">
      {/* ── Toolbar ── */}
      <div className="flex flex-wrap items-center px-1.5 py-1 bg-os-bg-base dark:bg-os-bg-base border-b border-os-border dark:border-os-border gap-0.5" role="toolbar" aria-label="文本格式化工具栏">
        {/* Undo / Redo */}
        <button className={btn} onClick={() => exec('undo')} title="撤销">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M3 7v6h6"/><path d="M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13"/></svg>
        </button>
        <button className={btn} onClick={() => exec('redo')} title="重做">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M21 7v6h-6"/><path d="M3 17a9 9 0 0 1 9-9 9 9 0 0 1 6 2.3L21 13"/></svg>
        </button>

        <span className="w-px h-5 mx-0.5 bg-os-border dark:bg-os-border" />

        {/* Text formatting */}
        <button className={`${btn} ${is('bold') ? 'bg-os-bg-hover text-os-text-primary font-bold' : 'text-os-text-secondary'}`}
          onClick={() => exec('bold')} title="加粗"><b>B</b></button>
        <button className={`${btn} ${is('italic') ? 'bg-os-bg-hover text-os-text-primary italic' : 'text-os-text-secondary'}`}
          onClick={() => exec('italic')} title="斜体"><i>I</i></button>
        <button className={`${btn} ${is('underline') ? 'bg-os-bg-hover text-os-text-primary underline' : 'text-os-text-secondary'}`}
          onClick={() => exec('underline')} title="下划线"><u>U</u></button>
        <button className={`${btn} ${is('strikeThrough') ? 'bg-os-bg-hover text-os-text-primary' : 'text-os-text-secondary'}`}
          onClick={() => exec('strikeThrough')} title="删除线"><s>S</s></button>

        <span className="w-px h-5 mx-0.5 bg-os-border dark:bg-os-border" />

        {/* Headings */}
        <button className={`${btn} text-xs ${is('formatBlock', 'h1') ? 'bg-os-bg-hover text-os-text-primary font-semibold' : 'text-os-text-secondary'}`}
          onClick={() => exec('formatBlock', 'h1')} title="标题1">H1</button>
        <button className={`${btn} text-xs ${is('formatBlock', 'h2') ? 'bg-os-bg-hover text-os-text-primary font-semibold' : 'text-os-text-secondary'}`}
          onClick={() => exec('formatBlock', 'h2')} title="标题2">H2</button>
        <button className={`${btn} text-xs ${is('formatBlock', 'h3') ? 'bg-os-bg-hover text-os-text-primary font-semibold' : 'text-os-text-secondary'}`}
          onClick={() => exec('formatBlock', 'h3')} title="标题3">H3</button>

        <span className="w-px h-5 mx-0.5 bg-os-border dark:bg-os-border" />

        {/* Lists */}
        <button className={`${btn} ${is('insertUnorderedList') ? 'bg-os-bg-hover text-os-text-primary' : 'text-os-text-secondary'}`}
          onClick={() => exec('insertUnorderedList')} title="无序列表">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/></svg>
        </button>
        <button className={`${btn} ${is('insertOrderedList') ? 'bg-os-bg-hover text-os-text-primary' : 'text-os-text-secondary'}`}
          onClick={() => exec('insertOrderedList')} title="有序列表">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><line x1="10" y1="6" x2="21" y2="6"/><line x1="10" y1="12" x2="21" y2="12"/><line x1="10" y1="18" x2="21" y2="18"/><path d="M4 6h1v4"/><path d="M4 10h2"/><path d="M6 18H4c0-1 2-2 2-3s-1-1.5-2-1"/></svg>
        </button>

        <span className="w-px h-5 mx-0.5 bg-os-border dark:bg-os-border" />

        {/* Blockquote / link */}
        <button className={`${btn} ${is('formatBlock', 'blockquote') ? 'bg-os-bg-hover text-os-text-primary' : 'text-os-text-secondary'}`}
          onClick={() => exec('formatBlock', 'blockquote')} title="引用">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M3 21c3 0 7-1 7-8V5c0-1.25-.756-2.017-2-2H4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2 1 0 1 0 1 1v1c0 1-1 2-2 2s-1 .008-1 1.031V20c0 1 0 1 1 1z"/><path d="M15 21c3 0 7-1 7-8V5c0-1.25-.757-2.017-2-2h-4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2h.75c0 2.25.25 4-2.75 4v3c0 1 0 1 1 1z"/></svg>
        </button>

        {/* Link */}
        <div className="relative inline-flex items-center">
          <button className={`${btn} ${isLinkActive() ? 'bg-os-bg-hover text-os-text-primary' : 'text-os-text-secondary'}`}
            onClick={() => { setShowLinkInput(!showLinkInput); setLinkUrl('') }}
            title="链接">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>
          </button>
          {showLinkInput && (
            <div className="absolute top-full left-0 mt-1 z-10 flex items-center gap-1 bg-white dark:bg-os-bg-card shadow-os-card border border-os-border rounded-os-md p-1.5 min-w-[220px]">
              <input
                type="text"
                value={linkUrl}
                onChange={(e) => setLinkUrl(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && setLink()}
                placeholder="输入 URL..."
                className="flex-1 text-xs px-2 py-1 border border-os-border rounded outline-none focus:border-os-primary-500 bg-transparent"
                autoFocus
              />
              <button className="px-2 py-1 text-xs rounded bg-os-primary-500 text-white hover:opacity-90 transition-opacity" onClick={setLink}>确定</button>
            </div>
          )}
        </div>
      </div>

      {/* ── Editor Content ── */}
      <div
        ref={editorRef}
        contentEditable
        suppressContentEditableWarning
        className="min-h-[200px] px-4 py-3 text-sm text-os-text-primary focus:outline-none [&:empty:before]:content-[attr(data-placeholder)] [&:empty:before]:text-os-text-muted [&:empty:before]:pointer-events-none prose prose-sm max-w-none"
        data-placeholder={placeholder || '输入内容...'}
        onInput={emitChange}
        onBlur={emitChange}
        onPaste={handlePaste}
        onKeyDown={handleKeyDown}
        dangerouslySetInnerHTML={{ __html: value }}
      />
    </div>
  )
}
