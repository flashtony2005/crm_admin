import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Button, Input, toast } from '@heroui/react'
import { workflowsApi, type WorkflowDef, type WorkflowNode } from '../../api/cms'

export interface WorkflowEditorProps {
  /** 要编辑的工作流 id；为空表示新建后编辑（通常先建好再编辑） */
  workflowId: string
  onClose: () => void
  onSaved?: (wf: WorkflowDef) => void
}

/** 节点类型 → 展示元信息 */
const NODE_TYPES: {
  type: string
  label: string
  color: string
  hint: string
}[] = [
  { type: 'trigger', label: '触发', color: '#6366f1', hint: '工作流的起点（由工作流 trigger / event 决定）' },
  { type: 'notify', label: '通知', color: '#10b981', hint: '发送消息，message 支持 {字段} 模板' },
  { type: 'task', label: '任务', color: '#f59e0b', hint: '创建一条待办任务，title 为任务标题' },
  { type: 'delay', label: '延时', color: '#0ea5e9', hint: '等待一段时间后再继续（message 形如 "5m"/"1h"）' },
  { type: 'webhook', label: 'Webhook', color: '#ef4444', hint: '调用外部接口，message 为 URL' },
  { type: 'condition', label: '条件', color: '#a855f7', hint: '根据字段分流（message 为条件表达式）' },
]

const NODE_W = 200
const NODE_H = 76

function metaOf(type: string) {
  return NODE_TYPES.find((n) => n.type === type) ?? NODE_TYPES[0]
}

function genId() {
  return 'n_' + Math.random().toString(36).slice(2, 9)
}

/**
 * 把后端返回的 steps（可能是 JSON 字符串或数组，节点可能缺 id/x/y/next）
 * 规范化为完整节点数组。解析失败或空 → 返回 []。
 */
function normalizeSteps(steps: unknown): WorkflowNode[] {
  let arr: unknown = steps
  if (typeof steps === 'string' && steps.trim()) {
    try {
      arr = JSON.parse(steps)
    } catch {
      arr = null
    }
  }
  if (!Array.isArray(arr)) return []
  return arr.map((raw, i) => {
    const n = (raw ?? {}) as Partial<WorkflowNode> & { type?: string }
    const m = metaOf(n.type ?? 'task')
    return {
      id: n.id ?? genId(),
      type: n.type ?? 'task',
      label: n.label ?? m.label,
      x: typeof n.x === 'number' ? n.x : 80 + (i % 3) * 40,
      y: typeof n.y === 'number' ? n.y : 60 + Math.floor(i / 3) * 90,
      next: Array.isArray(n.next) ? n.next : [],
      ...(n.message !== undefined ? { message: n.message } : {}),
      ...(n.title !== undefined ? { title: n.title } : {}),
    }
  })
}

/**
 * 可视化节点编辑器：画布上增删节点、拖拽定位、连线串联，保存为
 * 后端 workflows.steps（JSON 数组，单元素即一个节点，兼容执行引擎）。
 */
export function WorkflowEditor({ workflowId, onClose, onSaved }: WorkflowEditorProps) {
  const [wf, setWf] = useState<WorkflowDef | null>(null)
  const [name, setName] = useState('')
  const [trigger, setTrigger] = useState('manual')
  const [event, setEvent] = useState('manual')
  const [nodes, setNodes] = useState<WorkflowNode[]>([])
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  // 连线模式：null=未连线；string=已选中起点节点 id
  const [linkFrom, setLinkFrom] = useState<string | null>(null)
  const [selected, setSelected] = useState<string | null>(null)
  const canvasRef = useRef<HTMLDivElement>(null)
  // 拖拽状态
  const dragRef = useRef<{ id: string; dx: number; dy: number } | null>(null)

  useEffect(() => {
    let alive = true
    ;(async () => {
      setLoading(true)
      try {
        const w = await workflowsApi.get(workflowId)
        if (!alive) return
        if (w) {
          setWf(w)
          setName(w.name)
          setTrigger(w.trigger)
          setEvent(w.event)
          // steps 可能是 JSON 字符串（空串）或数组（缺 id/x/y/next），统一规范化；
          // 无有效节点时给 1 个默认 trigger 起点
          const steps = normalizeSteps(w.steps)
          setNodes(
            steps.length > 0
              ? steps
              : [
                  {
                    id: genId(),
                    type: 'trigger',
                    label: '开始',
                    x: 80,
                    y: 60,
                    next: [],
                  },
                ],
          )
        }
      } catch (e) {
        if (alive) toast((e as Error).message, { variant: 'danger' })
      } finally {
        if (alive) setLoading(false)
      }
    })()
    return () => {
      alive = false
    }
  }, [workflowId])

  const addNode = useCallback((type: string) => {
    const m = metaOf(type)
    const id = genId()
    setNodes((ns) => [
      ...ns,
      {
        id,
        type,
        label: m.label,
        x: 120 + Math.random() * 240,
        y: 80 + Math.random() * 260,
        next: [],
      },
    ])
    setSelected(id)
  }, [])

  const updateNode = useCallback((id: string, patch: Partial<WorkflowNode>) => {
    setNodes((ns) => ns.map((n) => (n.id === id ? { ...n, ...patch } : n)))
  }, [])

  const removeNode = useCallback((id: string) => {
    setNodes((ns) => ns.filter((n) => n.id !== id).map((n) => ({ ...n, next: (n.next ?? []).filter((t) => t !== id) })))
    setSelected(null)
    if (linkFrom === id) setLinkFrom(null)
  }, [linkFrom])

  // 连线：输出端口（右侧蓝点）选起点 → 输入端口（左侧蓝点）完成连线
  const handlePortClick = useCallback(
    (id: string) => {
      if (linkFrom === null) {
        setLinkFrom(id)
        return
      }
      if (linkFrom === id) {
        setLinkFrom(null)
        return
      }
      setNodes((ns) =>
        ns.map((n) => {
          if (n.id !== linkFrom) return n
          const next = n.next ?? []
          return { ...n, next: next.includes(id) ? next : [...next, id] }
        }),
      )
      setLinkFrom(null)
    },
    [linkFrom],
  )

  // 输入端口点击：完成「起点 → 当前节点」的连线
  const handleInputClick = useCallback(
    (id: string) => {
      if (linkFrom === null || linkFrom === id) return
      setNodes((ns) =>
        ns.map((n) => {
          if (n.id !== linkFrom) return n
          const next = n.next ?? []
          return { ...n, next: next.includes(id) ? next : [...next, id] }
        }),
      )
      setLinkFrom(null)
    },
    [linkFrom],
  )

  const removeEdge = useCallback((from: string, to: string) => {
    setNodes((ns) => ns.map((n) => (n.id === from ? { ...n, next: (n.next ?? []).filter((t) => t !== to) } : n)))
  }, [])

  // 拖拽
  const onNodeMouseDown = useCallback(
    (e: React.MouseEvent, id: string) => {
      e.preventDefault()
      const node = nodes.find((n) => n.id === id)
      if (!node || !canvasRef.current) return
      const rect = canvasRef.current.getBoundingClientRect()
      dragRef.current = { id, dx: e.clientX - rect.left - node.x, dy: e.clientY - rect.top - node.y }
      setSelected(id)
    },
    [nodes],
  )

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      const d = dragRef.current
      if (!d || !canvasRef.current) return
      const rect = canvasRef.current.getBoundingClientRect()
      const x = Math.max(0, e.clientX - rect.left - d.dx)
      const y = Math.max(0, e.clientY - rect.top - d.dy)
      setNodes((ns) => ns.map((n) => (n.id === d.id ? { ...n, x, y } : n)))
    }
    const onUp = () => {
      dragRef.current = null
    }
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
    return () => {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
    }
  }, [])

  const save = useCallback(async () => {
    setSaving(true)
    try {
      const body: Partial<WorkflowDef> = {
        name,
        trigger,
        event,
        stepCount: nodes.length,
        steps: nodes,
      }
      const updated = await workflowsApi.update(workflowId, body)
      toast('流程图已保存', { variant: 'success' })
      onSaved?.(updated)
      onClose()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    } finally {
      setSaving(false)
    }
  }, [workflowId, name, trigger, event, nodes, onClose, onSaved])

  // 计算连线坐标（从父节点右侧中点 → 子节点左侧中点）
  const edges = useMemo(() => {
    const out: { id: string; from: WorkflowNode; to: WorkflowNode; fromId: string; toId: string }[] = []
    const map = new Map(nodes.map((n) => [n.id, n]))
    for (const n of nodes) {
      for (const t of n.next ?? []) {
        const tn = map.get(t)
        if (tn) out.push({ id: `${n.id}->${t}`, from: n, to: tn, fromId: n.id, toId: t })
      }
    }
    return out
  }, [nodes])

  if (loading) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
        <div className="rounded-lg bg-white p-6 text-sm text-default-600 shadow-xl">加载中…</div>
      </div>
    )
  }

  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-default-50">
      {/* 顶栏 */}
      <div className="flex items-center justify-between border-b border-default-200 bg-white px-4 py-3">
        <div className="flex items-center gap-3">
          <span className="text-sm font-semibold">可视化节点编辑器</span>
          <Input
            size="sm"
            className="w-56"
            value={name}
            onValueChange={setName}
            placeholder="工作流名称"
          />
          <select
            className="rounded-md border border-default-300 px-2 py-1.5 text-sm"
            value={trigger}
            onChange={(e) => setTrigger(e.target.value)}
          >
            <option value="manual">手动触发</option>
            <option value="event">事件触发</option>
            <option value="schedule">定时触发</option>
          </select>
          <select
            className="rounded-md border border-default-300 px-2 py-1.5 text-sm"
            value={event}
            onChange={(e) => setEvent(e.target.value)}
          >
            <option value="manual">手动</option>
            <option value="customer.created">新客户创建时</option>
            <option value="article.published">文章发布后</option>
            <option value="schedule.weekly">每周定时</option>
          </select>
        </div>
        <div className="flex items-center gap-2">
          <Button size="sm" variant="ghost" onPress={onClose}>
            关闭
          </Button>
          <Button size="sm" variant="primary" isLoading={saving} onPress={save}>
            保存流程图
          </Button>
        </div>
      </div>

      {/* 独立画布容器：上方节点库 + 下方画布 */}
      <div className="relative m-3 flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-default-300 bg-white shadow-md">
        {/* 节点库（画布上方，全部节点类型一览） */}
        <div className="flex flex-wrap items-center gap-2 border-b border-default-200 bg-gray-50/80 px-4 py-2.5">
          <span className="text-xs font-semibold text-default-600">节点库</span>
          {NODE_TYPES.map((t) => (
            <button
              key={t.type}
              type="button"
              title={t.hint}
              onClick={() => addNode(t.type)}
              className="flex items-center gap-1.5 rounded-lg border bg-white px-2.5 py-1.5 text-xs font-medium shadow-sm transition-all hover:-translate-y-0.5 hover:shadow"
              style={{ borderColor: t.color, color: t.color }}
            >
              <span className="inline-block h-2 w-2 rounded-full" style={{ background: t.color }} />
              + {t.label}
            </button>
          ))}
          <span className="ml-auto text-xs text-default-400">
            {linkFrom
              ? '已选起点（红点），点击目标节点左侧蓝点完成连线 · 再点红点取消'
              : '连线：点节点右侧蓝点 → 再点另一节点左侧蓝点'}
          </span>
        </div>

        {/* 画布 */}
        <div
          ref={canvasRef}
          className="relative flex-1 overflow-auto bg-[radial-gradient(circle,rgba(0,0,0,0.08)_1px,transparent_1px)] [background-size:20px_20px]"
          style={{ backgroundPosition: '0 0' }}
        >
        {/* 连线层（SVG 在节点下方） */}
        <svg className="pointer-events-none absolute inset-0 h-full w-full" style={{ zIndex: 1 }}>
          {edges.map((e) => {
            const x1 = e.from.x + NODE_W
            const y1 = e.from.y + NODE_H / 2
            const x2 = e.to.x
            const y2 = e.to.y + NODE_H / 2
            const mx = (x1 + x2) / 2
            return (
              <g key={e.id}>
                <path
                  d={`M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`}
                  stroke="#64748b"
                  strokeWidth={2.5}
                  fill="none"
                  markerEnd="url(#arrow)"
                />
                <circle
                  cx={(x1 + x2) / 2}
                  cy={(y1 + y2) / 2}
                  r={7}
                  fill="#ef4444"
                  className="pointer-events-auto cursor-pointer"
                  onClick={() => removeEdge(e.fromId, e.toId)}
                >
                  <title>点击删除连线</title>
                </circle>
              </g>
            )
          })}
          <defs>
            <marker id="arrow" markerWidth={16} markerHeight={16} refX={12} refY={5} orient="auto" markerUnits="strokeWidth">
              <path d="M0,0 L0,10 L14,5 z" fill="#64748b" />
            </marker>
          </defs>
        </svg>

        {/* 节点 */}
        {nodes.map((n) => {
          const m = metaOf(n.type)
          const isSel = selected === n.id
          const isLinkSrc = linkFrom === n.id
          return (
            <div
              key={n.id}
              className={`group absolute select-none rounded-lg border bg-white shadow-sm transition-shadow ${
                isSel ? 'ring-2 ring-primary' : ''
              } ${isLinkSrc ? 'ring-2 ring-danger' : ''} ${
                linkFrom && linkFrom !== n.id ? 'ring-2 ring-[#6366F1] ring-offset-1' : ''
              }`}
              style={{ left: n.x, top: n.y, width: NODE_W, minHeight: NODE_H, borderColor: m.color }}
              onMouseDown={(e) => onNodeMouseDown(e, n.id)}
            >
              <div
                className="flex items-center justify-between rounded-t-lg px-2 py-1 text-xs font-medium text-white"
                style={{ background: m.color }}
              >
                <span>{m.label}</span>
                <button
                  className="text-white/80 hover:text-white"
                  onClick={(e) => {
                    e.stopPropagation()
                    removeNode(n.id)
                  }}
                >
                  ✕
                </button>
              </div>
              <div className="px-2 py-1.5">
                <input
                  className="w-full text-sm font-medium outline-none"
                  value={n.label}
                  placeholder="节点名称"
                  onChange={(e) => updateNode(n.id, { label: e.target.value })}
                  onMouseDown={(e) => e.stopPropagation()}
                />
                {(n.type === 'notify' || n.type === 'task' || n.type === 'webhook' || n.type === 'delay' || n.type === 'condition') && (
                  <input
                    className="mt-1 w-full text-xs text-default-500 outline-none"
                    value={n.message ?? ''}
                    placeholder={m.hint}
                    onChange={(e) => updateNode(n.id, { message: e.target.value })}
                    onMouseDown={(e) => e.stopPropagation()}
                  />
                )}
              </div>
              {/* 输出端口（连线起点：右侧蓝点，点击进入连线模式） */}
              <div
                title={linkFrom === n.id ? '再次点击取消连线模式' : '连线起点：点击进入连线模式，再点目标节点左侧蓝点完成连线'}
                className={`absolute -right-3 top-1/2 flex h-6 w-6 -translate-y-1/2 cursor-crosshair items-center justify-center rounded-full border-2 border-white shadow-sm transition-all group-hover:h-7 group-hover:w-7 ${
                  linkFrom === n.id
                    ? 'bg-red-500 ring-2 ring-red-200'
                    : 'bg-[#6366F1] hover:ring-2 hover:ring-indigo-200'
                }`}
                onMouseDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.stopPropagation()
                  handlePortClick(n.id)
                }}
              >
                <span className={`h-2 w-2 rounded-full ${linkFrom === n.id ? 'bg-white' : 'bg-white/85'}`} />
              </div>
              {/* 输入端口（连线终点：左侧蓝点，连线模式下点击完成连线） */}
              <div
                title={linkFrom && linkFrom !== n.id ? '点击完成连线' : '连线终点'}
                className={`absolute -left-3 top-1/2 flex h-6 w-6 -translate-y-1/2 cursor-pointer items-center justify-center rounded-full border-2 border-white shadow-sm transition-all group-hover:h-7 group-hover:w-7 ${
                  linkFrom && linkFrom !== n.id
                    ? 'bg-[#6366F1] ring-2 ring-indigo-200'
                    : 'bg-slate-400 hover:bg-[#6366F1]'
                }`}
                onMouseDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.stopPropagation()
                  handleInputClick(n.id)
                }}
              >
                <span className="h-1.5 w-1.5 rounded-full bg-white/85" />
              </div>
            </div>
          )
        })}

        {nodes.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center text-sm text-default-400">
            从上方「节点库」添加节点开始设计工作流
          </div>
        )}
      </div>
      </div>

      {/* 底部说明 */}
      <div className="border-t border-default-200 bg-white px-4 py-2 text-xs text-default-400">
        连线方法：点 A 节点右侧「蓝点」→ 再点 B 节点左侧「蓝点」，即连出 A→B 的箭头线；点连线中点红点可删除；点起点红点可取消连线模式。
      </div>
    </div>
  )
}
