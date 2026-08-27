/**
 * 由托管上传图 URL 推导响应式 srcset（与后端 upload.rs 生成的
 * 480/960/1600 变体命名一致）。
 *
 * 规则：仅对形如 `/uploads/<uuid>.<ext>` 或 `<PUBLIC_BASE_URL>/uploads/<uuid>.<ext>`
 * 的本站托管图片生成；外链 / data URL 返回空串（调用方不渲染 srcSet）。
 */
export function buildSrcset(url?: string | null): string {
  if (!url || url.startsWith('data:')) return ''
  const idx = url.lastIndexOf('/')
  const fname = idx >= 0 ? url.slice(idx + 1) : url
  const dot = fname.lastIndexOf('.')
  if (dot <= 0) return ''
  const stem = fname.slice(0, dot)
  const ext = fname.slice(dot + 1).toLowerCase()
  if (!['jpg', 'jpeg', 'png', 'gif', 'webp'].includes(ext)) return ''
  const head = idx >= 0 ? url.slice(0, idx + 1) : ''
  return [480, 960, 1600].map((w) => `${head}${stem}_${w}.${ext} ${w}w`).join(', ')
}
