/**
 * Lightweight whitelist HTML sanitizer for CMS article body rendering.
 *
 * Same core mechanism as DOMPurify: parse HTML into a DOM tree, then walk it
 * and drop any tag / attribute that is not on the whitelist. Sufficient for
 * the trusted-editor-but-untrusted-content model of a public blog (users can
 * only enter HTML via our own Markdown/rich-text editors, but we still must
 * neutralize anything that slips through, e.g. crafted <script>/<iframe> or
 * javascript: URLs).
 *
 * Notes:
 * - Runs in the browser (uses DOMParser), so it only sanitizes string -> string.
 * - CSS values are filtered by whitelisting style *properties*; we keep the
 *   few layout props our editors emit (image sizing, alignment, spacing).
 * - URL protocols are restricted to http(s):, mailto:, tel: and data:image/*
 *   (the editor inlines compressed images as data URLs).
 */

const ALLOWED_TAGS = new Set([
  'p', 'br', 'hr', 'strong', 'b', 'em', 'i', 'u', 's', 'del', 'sub', 'sup',
  'blockquote', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
  'ul', 'ol', 'li', 'span', 'div', 'code', 'pre',
  'a', 'img', 'table', 'thead', 'tbody', 'tfoot', 'tr', 'th', 'td',
  'figure', 'figcaption', 'section', 'mark',
])

// Attributes allowed per tag. Use '*' for attributes allowed on any tag.
const ALLOWED_ATTRS: Record<string, Set<string>> = {
  '*': new Set(['class', 'title']),
  a: new Set(['href', 'target', 'rel']),
  img: new Set(['src', 'alt', 'title', 'width', 'height', 'loading']),
  td: new Set(['colspan', 'rowspan']),
  th: new Set(['colspan', 'rowspan']),
  ol: new Set(['start', 'type']),
  li: new Set(['value']),
}

// Style properties we keep (layout only, no injection surface).
const ALLOWED_CSS = new Set([
  'max-width', 'width', 'height', 'border-radius', 'margin', 'padding',
  'display', 'text-align', 'font-size', 'font-weight', 'font-style',
  'line-height', 'color', 'background', 'background-color', 'float',
])

const SAFE_PROTOCOLS = ['http:', 'https:', 'mailto:', 'tel:', 'data:']

function isSafeUrl(raw: string, forImage: boolean): boolean {
  const v = raw.trim().toLowerCase()
  if (v.startsWith('#') || v.startsWith('/')) return true
  for (const p of SAFE_PROTOCOLS) {
    if (v.startsWith(p)) {
      // data: URLs only for images, and only image mime types
      if (p === 'data:' && !forImage) return false
      if (p === 'data:' && !/^data:image\//.test(v)) return false
      return true
    }
  }
  return false
}

function cleanStyle(raw: string): string {
  const out: string[] = []
  for (const decl of raw.split(';')) {
    const idx = decl.indexOf(':')
    if (idx <= 0) continue
    const prop = decl.slice(0, idx).trim().toLowerCase()
    const val = decl.slice(idx + 1).trim()
    if (!ALLOWED_CSS.has(prop)) continue
    // no url(...) / expression() / var(--...) in kept values
    if (/url\s*\(|expression\s*\(|javascript:|var\s*\(/i.test(val)) continue
    out.push(`${prop}: ${val}`)
  }
  return out.join('; ')
}

/** Sanitize untrusted HTML; returns clean markup safe for innerHTML. */
export function sanitizeHtml(raw: string): string {
  if (!raw) return ''
  let doc: Document
  try {
    doc = new DOMParser().parseFromString(raw, 'text/html')
  } catch {
    // Fallback: escape everything if DOM parsing is unavailable
    return raw
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
  }

  const walk = (node: Node) => {
    const children = Array.from(node.childNodes)
    for (const child of children) {
      if (child.nodeType === Node.ELEMENT_NODE) {
        const el = child as HTMLElement
        const tag = el.tagName.toLowerCase()
        if (!ALLOWED_TAGS.has(tag)) {
          // Drop the node entirely (do not unwrap; scripts/iframes must vanish)
          el.remove()
          continue
        }
        // Strip attributes
        for (const attr of Array.from(el.attributes)) {
          const name = attr.name.toLowerCase()
          const allowed = ALLOWED_ATTRS['*'].has(name) || ALLOWED_ATTRS[tag]?.has(name)
          if (!allowed) {
            el.removeAttribute(attr.name)
            continue
          }
          if (name === 'href' || name === 'src') {
            if (!isSafeUrl(attr.value, tag === 'img')) {
              el.removeAttribute(attr.name)
              continue
            }
          }
          if (name === 'target' && attr.value !== '_blank') {
            el.removeAttribute(attr.name)
            continue
          }
          if (name === 'rel' && !/noopener|noreferrer/.test(attr.value)) {
            el.setAttribute('rel', 'noopener noreferrer')
            continue
          }
          if (name === 'class' && !/^[a-zA-Z0-9_ -]*$/.test(attr.value)) {
            el.removeAttribute(attr.name)
            continue
          }
        }
        // Ensure links with target=_blank carry rel
        if (tag === 'a' && el.getAttribute('target') === '_blank') {
          el.setAttribute('rel', 'noopener noreferrer')
        }
        // Images must have safe src
        if (tag === 'img') {
          const src = el.getAttribute('src') || ''
          if (!isSafeUrl(src, true)) el.removeAttribute('src')
          if (!el.getAttribute('alt')) el.setAttribute('alt', '')
        }
        // Style attribute: keep only whitelisted properties
        if (el.hasAttribute('style')) {
          const cleaned = cleanStyle(el.getAttribute('style') || '')
          if (cleaned) el.setAttribute('style', cleaned)
          else el.removeAttribute('style')
        }
        walk(el)
      } else if (child.nodeType === Node.COMMENT_NODE) {
        // Drop comments (avoid conditional comments / hidden payloads)
        child.remove()
      }
    }
  }

  walk(doc.body)
  return doc.body.innerHTML
}

export default sanitizeHtml
