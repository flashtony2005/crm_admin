import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, toast } from '@heroui/react'

import { workflowsApi, type WorkflowDef, CMS_MODE } from '../../api/cms'
import { request } from '../../api/client'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { fmtRelative } from '../../components/cms/format'
import { usePermission } from '../../hooks/usePermission'
import { P } from '../../config/permissions'
import { createFileRoute } from '@tanstack/react-router'
import { WorkflowEditor } from './WorkflowEditor'

/** 可订阅的事件类型（服务端 automation::trigger 的入口事件） */
const EVENT_OPTIONS: { value: string; label: string }[] = [
  { value: 'customer.created', label: '新客户创建时' },
  { value: 'article.published', label: '文章发布后' },
  { value: 'schedule.weekly', label: '每周定时（周一 08:00）' },
  { value: 'manual', label: '手动触发' },
]

interface RunRow {
  id: string
  detail: string
  createdAt: string
}

function WorkflowsPage() {
  const qc = useQueryClient()
  const { has } = usePermission()
  const canToggle = has(P.automationWorkflowsToggle)
  const isReal = CMS_MODE === 'real'
  // 编辑器状态：null=关闭；{id?}=新建或编辑
  const [editing, setEditing] = useState<{ id?: string; name: string; event: string } | null>(null)
  // 可视化节点编辑器：null=关闭；string=正在编辑的工作流 id
  const [editingFlow, setEditingFlow] = useState<string | null>(null)

  const { data: list = [], isLoading } = useQuery({
    queryKey: ['cms-workflows'],
    queryFn: () => workflowsApi.list(),
  })

  // 最近运行记录（real：审计流水过滤工作流运行）
  const { data: runs = [] } = useQuery({
    queryKey: ['workflow-runs'],
    enabled: isReal,
    refetchInterval: 30_000,
    queryFn: async (): Promise<RunRow[]> => {
      const body = await request<{ ok: boolean; data: (RunRow & { capability: string })[] }>('/api/ai/audit')
      return (body.data ?? [])
        .filter((a) => a.capability === 'automation.workflow.run')
        .slice(0, 8)
    },
  })

  // 启停切换
  const toggle = useMutation({
    mutationFn: (w: WorkflowDef) => workflowsApi.update(w.id, { enabled: !w.enabled }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['cms-workflows'] }),
  })

  const save = useMutation({
    mutationFn: (input: { id?: string; name: string; event: string }) => {
      const trigger = EVENT_OPTIONS.find((e) => e.value === input.event)?.label ?? input.event
      if (input.id) return workflowsApi.update(input.id, { name: input.name, trigger, event: input.event })
      return workflowsApi.create({ name: input.name, trigger, event: input.event, stepCount: 1, enabled: true })
    },
    onSuccess: (_d, input) => {
      toast.success(input.id ? '流程已更新' : '流程已创建并启用')
      setEditing(null)
      void qc.invalidateQueries({ queryKey: ['cms-workflows'] })
    },
  })

  const eventLabel = (w: WorkflowDef) =>
    EVENT_OPTIONS.find((e) => e.value === w.event)?.label ?? w.event ?? w.trigger

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader
        title="Workflows"
        desc="把重复的事交给自动化：欢迎邮件、经营摘要、跟进提醒。"
        actions={
          <button
            type="button"
            disabled={!canToggle}
            onClick={() => setEditing({ name: '', event: 'customer.created' })}
            title={canToggle ? undefined : `需要权限：${P.automationWorkflowsToggle}`}
            className="h-8 px-3 rounded-lg bg-[#4F46E5] text-white text-sm font-medium disabled:opacity-40 hover:bg-[#4338CA] transition-colors"
          >
            + 新建流程
          </button>
        }
      />

      {isLoading ? (
        <p className="text-sm text-os-text-muted py-16 text-center">加载中…</p>
      ) : (
        <ul className="flex flex-col gap-2.5">
          {list.map((w) => (
            <li
              key={w.id}
              className="rounded-xl border border-os-border bg-white p-4 shadow-sm flex items-center gap-4 flex-wrap"
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 flex-wrap">
                  <h3 className="text-sm font-medium text-os-text-primary">{w.name}</h3>
                  <span
                    className={`text-xs px-2 py-0.5 rounded-md font-medium ${
                      w.enabled ? 'bg-green-50 text-green-600' : 'bg-gray-100 text-gray-400'
                    }`}
                  >
                    {w.enabled ? '运行中' : '已暂停'}
                  </span>
                  {isReal && (
                    <span className="text-xs px-2 py-0.5 rounded-md bg-indigo-50 text-indigo-500 font-mono">
                      {w.event}
                    </span>
                  )}
                </div>
                <p className="text-xs text-os-text-muted mt-1">
                  触发：{isReal ? eventLabel(w) : w.trigger} · {w.stepCount} 个步骤
                  {w.lastRunAt && ` · 最近运行 ${fmtRelative(w.lastRunAt)}`}
                </p>
              </div>

              {/* 编辑（real 且有权） */}
              {isReal && canToggle && (
                <Button
                  variant="ghost" size="sm"
                  onPress={() =>
                    setEditing({
                      id: w.id,
                      name: w.name,
                      event: w.event || 'manual',
                    })
                  }
                >
                  编辑
                </Button>
              )}

              {/* 可视化节点编辑器入口（real 且有权） */}
              {isReal && canToggle && (
                <Button
                  variant="ghost" size="sm"
                  onPress={() => setEditingFlow(w.id)}
                >
                  编辑流程图
                </Button>
              )}

              {/* 启停开关 */}
              <button
                type="button"
                role="switch"
                aria-checked={w.enabled}
                aria-label={`切换 ${w.name}`}
                title={canToggle ? undefined : `需要权限：${P.automationWorkflowsToggle}`}
                disabled={toggle.isPending || !canToggle}
                onClick={() => toggle.mutate(w)}
                className={`relative w-10 h-[22px] rounded-full transition-colors flex-shrink-0 ${
                  w.enabled ? 'bg-[#6366F1]' : 'bg-gray-300'
                }`}
              >
                <span
                  className={`absolute top-[3px] w-4 h-4 rounded-full bg-white shadow transition-all ${
                    w.enabled ? 'left-[22px]' : 'left-[3px]'
                  }`}
                />
              </button>
            </li>
          ))}
        </ul>
      )}

      {/* 最近运行（real 模式，取自审计流水） */}
      {isReal && runs.length > 0 && (
        <section className="mt-6">
          <h2 className="text-sm font-medium text-os-text-secondary mb-2">最近运行</h2>
          <ul className="flex flex-col gap-1.5">
            {runs.map((r) => (
              <li key={r.id} className="rounded-lg bg-white border border-os-border-light px-3.5 py-2.5 flex items-center gap-3">
                <span className="w-1.5 h-1.5 rounded-full bg-green-500 flex-shrink-0" />
                <span className="text-xs text-os-text-primary flex-1 min-w-0 truncate">{r.detail}</span>
                <span className="text-xs text-os-text-muted flex-shrink-0">{fmtRelative(r.createdAt)}</span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {/* 新建 / 编辑弹窗（Phase 5 工作流编辑器 MVP） */}
      {editing && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4" role="dialog" aria-modal>
          <div className="rounded-2xl bg-white shadow-xl w-full max-w-sm p-5">
            <h3 className="text-base font-semibold text-os-text-primary">
              {editing.id ? '编辑流程' : '新建流程'}
            </h3>
            <div className="flex flex-col gap-3 mt-3">
              <div>
                <label htmlFor="wf-name" className="text-xs text-os-text-muted">名称</label>
                <input
                  id="wf-name" autoFocus value={editing.name}
                  onChange={(e) => setEditing({ ...editing, name: e.target.value })}
                  placeholder="如 新客户欢迎消息"
                  className="w-full h-10 mt-1 px-3 text-sm rounded-lg border border-os-border outline-none focus:border-[#6366F1]"
                />
              </div>
              <div>
                <label htmlFor="wf-event" className="text-xs text-os-text-muted">触发事件</label>
                <select
                  id="wf-event" value={editing.event}
                  onChange={(e) => setEditing({ ...editing, event: e.target.value })}
                  className="w-full h-10 mt-1 px-2 text-sm rounded-lg border border-os-border bg-white outline-none focus:border-[#6366F1]"
                >
                  {EVENT_OPTIONS.map((o) => (
                    <option key={o.value} value={o.value}>{o.label}</option>
                  ))}
                </select>
              </div>
              {!editing.id && (
                <p className="text-xs text-os-text-muted">创建后立即启用；可随时用开关暂停。</p>
              )}
            </div>
            <div className="flex justify-end gap-2 mt-4">
              <Button variant="ghost" size="sm" onPress={() => setEditing(null)}>取消</Button>
              <Button
                variant="primary" size="sm"
                isDisabled={!editing.name.trim() || save.isPending}
                onPress={() => save.mutate(editing)}
              >
                {save.isPending ? '保存中…' : '保存'}
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* 可视化节点编辑器（全屏） */}
      {editingFlow && (
        <WorkflowEditor
          workflowId={editingFlow}
          onClose={() => setEditingFlow(null)}
          onSaved={() => qc.invalidateQueries({ queryKey: ['cms-workflows'] })}
        />
      )}
    </div>
  )
}

export const Route = createFileRoute('/automation/workflows')({
  component: WorkflowsPage,
})
