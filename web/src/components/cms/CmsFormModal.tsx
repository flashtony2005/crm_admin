import { useEffect, useMemo, useState } from 'react'
import { Button, Modal } from '@heroui/react'
import type { FormFieldDef } from '../../api/cms'
import { RichTextEditor } from './RichTextEditor'

export type FormValues = Record<string, string | number>

interface Props {
  title: string
  isOpen: boolean
  onClose: () => void
  onSubmit: (values: FormValues) => Promise<void> | void
  /** 字段 schema（声明式，页面零自定义代码） */
  fields: FormFieldDef[]
  /** 编辑回填：任意实体对象（按键名取值） */
  initial?: object | null
  submitLabel?: string
}

const inputCls =
  'w-full border border-gray-200 rounded-lg px-3 py-2 text-sm outline-none focus:border-indigo-400 focus:ring-2 focus:ring-indigo-100 transition-all bg-white'

function toFormState(
  fields: FormFieldDef[],
  initial?: object | null,
): FormValues {
  const src = (initial ?? {}) as Record<string, unknown>
  const out: FormValues = {}
  for (const f of fields) {
    const v = src[f.key]
    out[f.key] = v !== undefined && v !== null ? (v as string | number) : (f.defaultValue ?? '')
  }
  return out
}

/**
 * CMS 表单弹窗（抽象组件）：按字段 schema 渲染，创建/编辑共用。
 */
export function CmsFormModal({
  title,
  isOpen,
  onClose,
  onSubmit,
  fields,
  initial,
  submitLabel = '保存',
}: Props) {
  const [values, setValues] = useState<FormValues>(() => toFormState(fields, initial))
  const [error, setError] = useState('')
  const [saving, setSaving] = useState(false)
  const hasRich = fields.some((f) => f.type === 'richtext')

  const modalState = useMemo(
    () => ({ isOpen, setOpen: onClose, open: () => {}, close: onClose, toggle: () => {} }),
    [isOpen, onClose],
  )

  // 打开时重置表单（创建空表单 / 编辑回填）
  useEffect(() => {
    if (isOpen) {
      setValues(toFormState(fields, initial))
      setError('')
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen])

  const setField = (key: string, v: string | number) =>
    setValues((prev) => ({ ...prev, [key]: v }))

  const handleSubmit = async () => {
    const missing = fields.find(
      (f) => f.required && String(values[f.key] ?? '').trim() === '',
    )
    if (missing) {
      setError(`请填写「${missing.label}」`)
      return
    }
    setSaving(true)
    try {
      await onSubmit(values)
      onClose()
    } catch (e) {
      setError(e instanceof Error ? e.message : '保存失败')
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal state={modalState}>
      <Modal.Backdrop>
        <Modal.Container placement="center">
          <Modal.Dialog className={`w-[${hasRich ? 800 : 480}px] max-w-[95vw]`}>
            <Modal.Header>
              <Modal.Heading>{title}</Modal.Heading>
              <Modal.CloseTrigger />
            </Modal.Header>
            <Modal.Body>
              <div className="flex flex-col gap-3.5 py-1 max-h-[72vh] overflow-y-auto pr-1">
                {fields.map((f) => (
                  <label key={f.key} className="flex flex-col gap-1.5">
                    <span className="text-sm font-medium text-gray-700">
                      {f.label}
                      {f.required && <span className="text-red-500 ml-0.5">*</span>}
                    </span>
                    {f.type === 'richtext' ? (
                      <RichTextEditor
                        value={String(values[f.key] ?? '')}
                        onChange={(html) => setField(f.key, html)}
                        placeholder={f.placeholder}
                        height={f.height}
                      />
                    ) : f.type === 'textarea' ? (
                      <textarea
                        rows={3}
                        value={String(values[f.key] ?? '')}
                        onChange={(e) => setField(f.key, e.target.value)}
                        placeholder={f.placeholder}
                        className={inputCls}
                      />
                    ) : f.type === 'select' ? (
                      <select
                        value={String(values[f.key] ?? '')}
                        onChange={(e) => setField(f.key, e.target.value)}
                        className={inputCls}
                      >
                        {(f.options ?? []).map((o) => (
                          <option key={o.value} value={o.value}>
                            {o.label}
                          </option>
                        ))}
                      </select>
                    ) : (
                      <input
                        type={f.type === 'number' ? 'number' : 'text'}
                        value={String(values[f.key] ?? '')}
                        onChange={(e) =>
                          setField(f.key, f.type === 'number' ? Number(e.target.value || 0) : e.target.value)
                        }
                        placeholder={f.placeholder}
                        className={inputCls}
                      />
                    )}
                  </label>
                ))}
                {error && <p className="text-sm text-red-500">{error}</p>}
              </div>
            </Modal.Body>
            <Modal.Footer>
              <Button variant="ghost" onPress={onClose}>
                取消
              </Button>
              <Button variant="primary" isDisabled={saving} onPress={handleSubmit}>
                {saving ? '保存中…' : submitLabel}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  )
}
