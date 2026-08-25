/** CMS 页面共享格式化工具 */

/** ISO 时间 → 短日期（MM-DD HH:mm），非法值回退 '—' */
export function fmtDate(iso?: string): string {
  if (!iso) return '—'
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return '—'
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getMonth() + 1}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** 文件大小（KB）→ 可读文案 */
export function fmtSize(kb: number): string {
  return kb >= 1024 ? `${(kb / 1024).toFixed(1)} MB` : `${kb} KB`
}

/** 相对时间（几分钟前 / 几天前） */
export function fmtRelative(iso?: string): string {
  if (!iso) return '—'
  const d = new Date(iso).getTime()
  if (Number.isNaN(d)) return '—'
  const diff = Date.now() - d
  const min = Math.floor(diff / 60000)
  if (min < 1) return '刚刚'
  if (min < 60) return `${min} 分钟前`
  const h = Math.floor(min / 60)
  if (h < 24) return `${h} 小时前`
  const day = Math.floor(h / 24)
  if (day < 30) return `${day} 天前`
  return `${Math.floor(day / 30)} 个月前`
}
