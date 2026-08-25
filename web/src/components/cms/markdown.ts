/**
 * 零依赖 Markdown ↔ HTML 转换（Ghost 风格写作体验的轻量实现）。
 *
 * 设计约束：
 * - 不引入第三方依赖，纯字符串/DOMParser 处理，React 19 兼容；
 * - 覆盖写作者 90% 需求：标题、粗体/斜体/删除线、行内代码、链接、图片、
 *   无序/有序列表、引用、围栏代码块、段落；
 * - HTML → Markdown 供"富文本切到源码"时保留内容；Markdown → HTML
 *   供保存时把源码渲染为正文（后端 content 仍存 HTML，阅读页零改动）。
 */

// ── Markdown → HTML ─────────────────────────────────────────

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

/** 行内语法 → HTML（按从长到短顺序替换，避免嵌套误伤） */
function inline(md: string): string {
  let s = escapeHtml(md)

  // 图片 ![alt](url)
  s = s.replace(/!\[([^\]]*)\]\(([^)\s]+)(?:\s+"([^"]*)")?\)/g, (_m, alt, url) => {
    const u = url.replace(/&amp;/g, '&')
    return `<img src="${u}" alt="${alt || ''}" loading="lazy" style="max-width:100%;height:auto;border-radius:8px;margin:8px 0;display:block;" />`
  })
  // 链接 [text](url)
  s = s.replace(/\[([^\]]+)\]\(([^)\s]+)(?:\s+"([^"]*)")?\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>')
  // 行内代码 `code`（先于粗体/斜体，避免 * 在 code 内被转义）
  s = s.replace(/`([^`]+)`/g, '<code style="background:#f3f4f6;padding:2px 6px;border-radius:4px;font-size:0.92em;">$1</code>')
  // **bold** / __bold__
  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  // *italic* / _italic_
  s = s.replace(/(^|[^*])\*([^*\n]+)\*(?!\*)/g, '$1<em>$2</em>')
  // ~~删除线~~
  s = s.replace(/~~([^~]+)~~/g, '<del>$1</del>')
  return s
}

/** 块级解析：Markdown 文本 → HTML 片段 */
export function markdownToHtml(md: string): string {
  if (!md || !md.trim()) return ''
  const lines = md.replace(/\r\n/g, '\n').split('\n')
  const out: string[] = []
  let i = 0

  const flushList = (tag: 'ul' | 'ol', items: string[]) => {
    if (items.length === 0) return
    out.push(`<${tag}>${items.map((it) => `<li>${inline(it)}</li>`).join('')}</${tag}>`)
  }

  let listTag: 'ul' | 'ol' | null = null
  let listItems: string[] = []

  while (i < lines.length) {
    const line = lines[i]

    // 围栏代码块 ```lang
    if (/^```/.test(line)) {
      flushList(listTag, listItems); listTag = null; listItems = []
      const buf: string[] = []
      i++
      while (i < lines.length && !/^```/.test(lines[i])) {
        buf.push(lines[i]); i++
      }
      i++ // 跳过结束围栏
      out.push(`<pre style="background:#0f172a;color:#e2e8f0;padding:12px 14px;border-radius:8px;overflow:auto;font-size:0.9em;"><code>${buf.join('\n')}</code></pre>`)
      continue
    }

    // 标题
    const h = line.match(/^(#{1,4})\s+(.*)$/)
    if (h) {
      flushList(listTag, listItems); listTag = null; listItems = []
      const level = h[1].length
      out.push(`<h${level}>${inline(h[2])}</h${level}>`)
      i++
      continue
    }

    // 引用
    if (/^>\s?/.test(line)) {
      flushList(listTag, listItems); listTag = null; listItems = []
      const buf: string[] = []
      while (i < lines.length && /^>\s?/.test(lines[i])) {
        buf.push(lines[i].replace(/^>\s?/, '')); i++
      }
      out.push(`<blockquote style="border-left:3px solid #c7d2fe;padding:2px 0 2px 14px;color:#64748b;margin:8px 0;">${buf.map((b) => inline(b)).join('<br/>')}</blockquote>`)
      continue
    }

    // 无序列表
    const ul = line.match(/^[-*+]\s+(.*)$/)
    // 有序列表
    const ol = line.match(/^\d+[.)]\s+(.*)$/)

    if (ul || ol) {
      const tag = ul ? 'ul' : 'ol'
      const content = ul ? ul[1] : ol![1]
      if (listTag !== tag) {
        flushList(listTag, listItems)
        listTag = tag; listItems = []
      }
      listItems.push(content)
      i++
      continue
    }

    // 其他：先收尾列表，再按空行分组段落
    flushList(listTag, listItems); listTag = null; listItems = []
    if (line.trim() === '') { i++; continue }
    const para: string[] = []
    while (i < lines.length && lines[i].trim() !== '' && !/^(#{1,4}\s|>|```|[-*+]\s|\d+[.)]\s)/.test(lines[i])) {
      para.push(lines[i].trim()); i++
    }
    out.push(`<p>${inline(para.join(' '))}</p>`)
  }
  flushList(listTag, listItems)

  return out.join('\n')
}

// ── HTML → Markdown ─────────────────────────────────────────

/** 节点 → 行内 Markdown 文本（递归收集 strong/em/a/code/img 语法） */
function nodeToInline(node: Node): string {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? ''
  if (node.nodeType !== Node.ELEMENT_NODE) return ''
  const el = node as HTMLElement
  const tag = el.tagName.toLowerCase()
  const inner = Array.from(el.childNodes).map(nodeToInline).join('')
  switch (tag) {
    case 'br': return '\n'
    case 'strong': case 'b': return `**${inner}**`
    case 'em': case 'i': return `*${inner}*`
    case 'del': case 's': return `~~${inner}~~`
    case 'code': return '`' + inner + '`'
    case 'a': {
      const href = el.getAttribute('href') || ''
      return href ? `[${inner}](${href})` : inner
    }
    case 'img': {
      const src = el.getAttribute('src') || ''
      const alt = el.getAttribute('alt') || ''
      return src ? `![${alt}](${src})` : ''
    }
    default: return inner
  }
}

/** 块级节点 → Markdown 行（返回多行字符串） */
function blockToMd(node: Node): string {
  if (node.nodeType === Node.ELEMENT_NODE) {
    const el = node as HTMLElement
    const tag = el.tagName.toLowerCase()
    const inlineText = Array.from(el.childNodes).map(nodeToInline).join('').trim()
    switch (tag) {
      case 'h1': return `# ${inlineText}`
      case 'h2': return `## ${inlineText}`
      case 'h3': return `### ${inlineText}`
      case 'h4': return `#### ${inlineText}`
      case 'blockquote':
        return Array.from(el.childNodes)
          .map(blockToMd)
          .filter(Boolean)
          .map((l) => '> ' + l)
          .join('\n')
      case 'pre': {
        const code = el.textContent ?? ''
        return '```\n' + code.replace(/\n$/, '') + '\n```'
      }
      case 'ul': case 'ol': {
        const ordered = tag === 'ol'
        return Array.from(el.children)
          .filter((li) => li.tagName.toLowerCase() === 'li')
          .map((li, idx) => `${ordered ? idx + 1 + '. ' : '- '}${Array.from(li.childNodes).map(nodeToInline).join('')}`)
          .join('\n')
      }
      case 'p': return inlineText
      case 'div': case 'section': case 'article': case 'body': case 'li':
        return Array.from(el.childNodes).map(blockToMd).filter(Boolean).join('\n')
      case 'img': {
        const src = el.getAttribute('src') || ''
        const alt = el.getAttribute('alt') || ''
        return src ? `![${alt}](${src})` : ''
      }
      case 'br': return ''
      default: {
        const txt = inlineText
        return txt && (tag === 'a' || tag === 'strong' || tag === 'em') ? txt : txt
      }
    }
  }
  const txt = (node.textContent ?? '').trim()
  return txt
}

/** 富文本 HTML → Markdown（供切换到源码模式时保留内容） */
export function htmlToMarkdown(html: string): string {
  if (!html || !html.trim()) return ''
  const doc = new DOMParser().parseFromString(html, 'text/html')
  return Array.from(doc.body.childNodes)
    .map(blockToMd)
    .filter((s) => s.trim() !== '')
    .join('\n\n')
}
