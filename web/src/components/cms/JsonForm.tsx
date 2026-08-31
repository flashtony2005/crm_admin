/**
 * 通用 JSON 结构表单（零依赖，按值类型递归渲染）
 *
 * 替代「整块 JSON 文本域」的编辑方式：运营人员按字段填写，不必接触 JSON 语法。
 *
 * - object → 字段分组（可增删字段，标签优先用中文映射）
 * - array  → 列表（可增删、▲▼ 排序，新增项按上一项结构克隆空壳）
 * - string → 单行输入；长文本（>60 字或含换行）自动切换多行文本域
 * - number → 数字输入；boolean → 开关；null → 空值可填
 * - 图片类字段（logo/cover/image…）附加「素材库」选择器，懒加载 /api/media
 *
 * 之所以不引入 JSON Schema 表单库（@jsonforms / rjsf）：首页区块结构不固定，
 * 按值推导对新旧区块都通用，且不必引入数百 KB 依赖与样式适配成本。
 */
import { useState } from 'react'
import { Button, Input, Switch, Label } from '@heroui/react'
import { api } from '../../api/client'

type Json = unknown

/** key → 中文标签（覆盖现有 about / org / lab / web3 四个区块的全部字段） */
const LABEL_ZH: Record<string, string> = {
  kicker: '眉标',
  tagline: '标语',
  hero: '主视觉',
  title: '标题',
  bio: '简介',
  ctaLabel: '按钮文字',
  ctaHref: '按钮链接',
  affiliation: '所属组织',
  eyebrow: '小标题',
  name: '名称',
  logo: '标志',
  desc: '描述',
  pillars: '支柱',
  items: '条目',
  no: '编号',
  writing: '代表文章',
  series: '系列',
  coming: '建设中',
  href: '链接',
  web3: 'Web3 区块',
  disclaimer: '免责声明',
  inviteCode: '邀请码',
  subtitle: '副标题',
  summary: '摘要',
  url: '地址',
  alt: '替代文字',
  target: '打开方式',
  enabled: '启用',
  sort: '排序',
}

/** 视为图片地址的字段名 */
const IMAGE_KEY = /^(logo|cover|image|img|avatar|thumb|thumbnail|poster|banner|photo|icon)$/i

const labelOf = (key: string) => LABEL_ZH[key] ?? key

function isObj(v: Json): v is Record<string, Json> {
  return !!v && typeof v === 'object' && !Array.isArray(v)
}

/** 按样本结构生成同构空壳（新增列表项时使用，避免复制大段文本） */
function blankOf(v: Json): Json {
  if (Array.isArray(v)) return []
  if (isObj(v)) {
    return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, blankOf(x)]))
  }
  if (typeof v === 'number') return 0
  if (typeof v === 'boolean') return false
  return ''
}

interface MediaItem {
  id: string
  name: string
  url: string
  thumbnail?: string | null
}

/** 素材选择器（懒加载 /api/media，点选即填入 URL） */
function MediaPicker({ onPick }: { onPick: (url: string) => void }) {
  const [open, setOpen] = useState(false)
  const [items, setItems] = useState<MediaItem[]>([])
  const [loading, setLoading] = useState(false)
  const [err, setErr] = useState('')

  const toggle = async () => {
    if (open) {
      setOpen(false)
      return
    }
    setOpen(true)
    if (items.length === 0 && !loading) {
      setLoading(true)
      setErr('')
      try {
        const rows = await api<MediaItem[]>('/api/media')
        setItems((rows || []).filter((m) => !!m.url))
      } catch (e) {
        setErr((e as Error).message)
      } finally {
        setLoading(false)
      }
    }
  }

  return (
    <div className="relative">
      <Button size="sm" variant="outline" onPress={toggle}>
        {open ? '收起' : '素材库'}
      </Button>
      {open && (
        <div className="absolute right-0 z-30 mt-1 w-72 rounded-xl border border-default-200 bg-default-50 p-2 shadow-lg">
          {loading && <p className="p-2 text-xs text-default-400">加载中…</p>}
          {err && <p className="p-2 text-xs text-danger">{err}</p>}
          {!loading && !err && items.length === 0 && (
            <p className="p-2 text-xs text-default-400">素材库为空，请先在「内容 → 素材」上传。</p>
          )}
          {items.length > 0 && (
            <div className="grid grid-cols-3 gap-1.5 max-h-56 overflow-auto">
              {items.map((m) => (
                <button
                  key={m.id}
                  type="button"
                  title={m.name || m.url}
                  onClick={() => {
                    onPick(m.url)
                    setOpen(false)
                  }}
                  className="aspect-square overflow-hidden rounded-lg border border-default-200 bg-default-100"
                >
                  <img
                    src={m.thumbnail || m.url}
                    alt={m.name || ''}
                    className="h-full w-full object-cover"
                  />
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  )
}

interface Props {
  value: Json
  onChange: (v: Json) => void
  depth?: number
  /** 叶子字段名（用于中文标签与图片判定） */
  fieldKey?: string
  /** 当前节点在整体结构中的路径，如 `hero.title`、`pillars.items` */
  path?: string
  /** 需要高亮定位的路径（布局热区点击时传入）；自身或其祖先命中即高亮 */
  highlightPath?: string
}

/** 路径命中判定：完全相等，或高亮路径是当前路径的祖先 */
function hitPath(current: string, target?: string): boolean {
  if (!target || !current) return false
  return current === target || current.startsWith(`${target}.`) || current.startsWith(`${target}[`)
}

export function JsonForm({ value, onChange, depth = 0, fieldKey, path = '', highlightPath }: Props) {
  const ring = hitPath(path, highlightPath)

  // ── 数组：可增删、可排序的列表 ──
  if (Array.isArray(value)) {
    const setIdx = (i: number, v: Json) => {
      const next = [...value]
      next[i] = v
      onChange(next)
    }
    const move = (i: number, d: -1 | 1) => {
      const j = i + d
      if (j < 0 || j >= value.length) return
      const next = [...value]
      ;[next[i], next[j]] = [next[j], next[i]]
      onChange(next)
    }
    const del = (i: number) => onChange(value.filter((_, k) => k !== i))
    const add = () => onChange([...value, blankOf(value[value.length - 1] ?? '')])

    return (
      <div className={`space-y-2 ${ring ? 'rounded-lg ring-2 ring-primary/40 p-1' : ''}`}>
        {value.map((item, i) => (
          <div
            key={i}
            className="rounded-lg border border-default-200 bg-default-50/50 p-2.5 space-y-1.5"
          >
            <div className="flex items-center justify-between">
              <span className="text-[11px] font-medium text-default-400">#{i + 1}</span>
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  aria-label="上移"
                  className="px-1 text-[11px] text-default-400 hover:text-default-700"
                  onClick={() => move(i, -1)}
                >
                  ▲
                </button>
                <button
                  type="button"
                  aria-label="下移"
                  className="px-1 text-[11px] text-default-400 hover:text-default-700"
                  onClick={() => move(i, 1)}
                >
                  ▼
                </button>
                <button
                  type="button"
                  className="px-1 text-[11px] text-danger"
                  onClick={() => del(i)}
                >
                  删除
                </button>
              </div>
            </div>
            <JsonForm
              value={item}
              onChange={(v) => setIdx(i, v)}
              depth={depth + 1}
              path={`${path}[${i}]`}
              highlightPath={highlightPath}
            />
          </div>
        ))}
        <Button size="sm" variant="outline" onPress={add}>
          + 添加一项
        </Button>
      </div>
    )
  }

  // ── 对象：字段分组 ──
  if (isObj(value)) {
    const entries = Object.entries(value)
    const setKey = (k: string, v: Json) => onChange({ ...value, [k]: v })
    const delKey = (k: string) => {
      const next = { ...value }
      delete next[k]
      onChange(next)
    }
    const addKey = () => {
      // 按「第一个元素类型」猜一个新字段类型，重名自动加序号
      const sample = entries.length ? entries[0][1] : ''
      let name = 'newField'
      let i = 1
      while (name in value) name = `newField${i++}`
      onChange({ ...value, [name]: blankOf(sample) })
    }

    return (
      <div
        className={`${depth > 0 ? 'space-y-2.5' : 'space-y-3'} ${
          ring ? 'rounded-lg ring-2 ring-primary/40 p-1' : ''
        }`}
      >
        {entries.map(([k, v]) => (
          <div key={k} className="space-y-1">
            <div className="flex items-center justify-between gap-2">
              <Label className="text-xs">
                {labelOf(k)}
                {labelOf(k) !== k && (
                  <span className="ml-1 text-[10px] text-default-300">{k}</span>
                )}
              </Label>
              <button type="button" className="text-[11px] text-danger" onClick={() => delKey(k)}>
                删除
              </button>
            </div>
            <JsonForm
              value={v}
              onChange={(nv) => setKey(k, nv)}
              depth={depth + 1}
              fieldKey={k}
              path={path ? `${path}.${k}` : k}
              highlightPath={highlightPath}
            />
          </div>
        ))}
        <Button size="sm" variant="outline" onPress={addKey}>
          + 添加字段
        </Button>
      </div>
    )
  }

  // ── 叶子：boolean ──
  if (typeof value === 'boolean') {
    return (
      <div className={ring ? 'inline-flex rounded-md ring-2 ring-primary/40 p-0.5' : 'inline-flex'}>
        <Switch isSelected={value} onChange={() => onChange(!value)} aria-label={fieldKey || '开关'} />
      </div>
    )
  }

  // ── 叶子：number ──
  if (typeof value === 'number') {
    return (
      <div className={ring ? 'rounded-md ring-2 ring-primary/40' : ''}>
        <Input
          type="number"
          value={String(value)}
          onChange={(e: any) => onChange(Number(e.target.value))}
          aria-label={fieldKey || '数值'}
        />
      </div>
    )
  }

  // ── 叶子：string / null ──
  const text = value == null ? '' : String(value)
  const isImage = !!fieldKey && IMAGE_KEY.test(fieldKey)
  const long = text.length > 60 || text.includes('\n')

  if (long) {
    return (
      <div className={ring ? 'rounded-md ring-2 ring-primary/40' : ''}>
        <textarea
          value={text}
          onChange={(e) => onChange(e.target.value)}
          rows={4}
          className="w-full rounded-lg border border-default-200 bg-default-50 p-2.5 text-sm leading-relaxed outline-none focus:border-primary"
        />
      </div>
    )
  }

  return (
    <div
      className={`flex items-center gap-2 ${
        ring ? 'rounded-md ring-2 ring-primary/40 p-0.5' : ''
      }`}
    >
      <Input
        value={text}
        onChange={(e: any) => onChange(e.target.value)}
        placeholder={value == null ? '（空）' : ''}
        className="flex-1"
        aria-label={fieldKey || '值'}
      />
      {isImage && <MediaPicker onPick={(url) => onChange(url)} />}
      {isImage && text && (
        <img src={text} alt="" className="h-8 w-8 rounded border border-default-200 object-cover" />
      )}
    </div>
  )
}
