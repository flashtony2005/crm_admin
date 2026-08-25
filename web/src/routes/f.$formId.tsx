import { useEffect, useState } from 'react'
import { createFileRoute, useParams } from '@tanstack/react-router'

/**
 * /f/{formId} —— 公开表单收集页（B4）。
 * 无需登录：访客填表 → POST /api/public/forms/{id}/submit →
 * 留档 form_submissions 并自动生成线索（leads，source=表单:标题）。
 */

interface PublicForm {
  title: string
  descr: string
}

function PublicFormPage({ formId }: { formId: string }) {
  const [form, setForm] = useState<PublicForm | null>(null)
  const [err, setErr] = useState<string | null>(null)
  const [done, setDone] = useState(false)
  const [busy, setBusy] = useState(false)
  const [fields, setFields] = useState({ name: '', phone: '', interest: '', note: '' })

  useEffect(() => {
    fetch(`/api/public/forms/${formId}`)
      .then((r) => r.json())
      .then((b) => {
        if (b.ok) setForm(b.data)
        else setErr(b.error ?? '表单不存在或未开放')
      })
      .catch(() => setErr('网络错误，请稍后重试'))
  }, [formId])

  const submit = async () => {
    setErr(null)
    if (!fields.name.trim()) return setErr('请填写姓名')
    setBusy(true)
    try {
      const r = await fetch(`/api/public/forms/${formId}/submit`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(fields),
      })
      const b = await r.json()
      if (!r.ok) {
        setErr(b.error ?? '提交失败')
      } else {
        setDone(true)
      }
    } catch {
      setErr('网络错误，请稍后重试')
    } finally {
      setBusy(false)
    }
  }

  if (err && !form) {
    return (
      <div className="min-h-screen bg-gray-50 flex items-center justify-center px-4">
        <div className="rounded-2xl bg-white shadow-sm border border-gray-200 p-6 max-w-sm w-full text-center">
          <p className="text-sm text-gray-500">{err}</p>
          <button
            onClick={() => { setErr(null); window.location.reload() }}
            className="mt-3 text-xs text-indigo-600"
          >
            重试
          </button>
        </div>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-gray-50 flex items-center justify-center px-4 py-10">
      <div className="rounded-2xl bg-white shadow-sm border border-gray-200 p-6 max-w-sm w-full">
        {!form ? (
          <p className="text-sm text-gray-400 text-center py-8">加载中…</p>
        ) : done ? (
          <div className="text-center py-6">
            <div className="text-5xl mb-3">🎉</div>
            <h2 className="text-base font-semibold text-gray-900">提交成功</h2>
            <p className="text-xs text-gray-500 mt-2">
              我们已收到你的信息，{fields.phone ? '会尽快电话联系你。' : '感谢关注！'}
            </p>
          </div>
        ) : (
          <>
            <h1 className="text-lg font-semibold text-gray-900">{form.title}</h1>
            <p className="text-xs text-gray-500 mt-1">{form.descr}</p>
            <div className="flex flex-col gap-3 mt-5">
              <input
                value={fields.name} onChange={(e) => setFields({ ...fields, name: e.target.value })}
                placeholder="姓名 *"
                className="h-11 px-3 rounded-xl border border-gray-300 bg-white text-base outline-none focus:border-indigo-500"
              />
              <input
                value={fields.phone} onChange={(e) => setFields({ ...fields, phone: e.target.value })}
                placeholder="手机号"
                type="tel"
                className="h-11 px-3 rounded-xl border border-gray-300 bg-white text-base outline-none focus:border-indigo-500"
              />
              <input
                value={fields.interest} onChange={(e) => setFields({ ...fields, interest: e.target.value })}
                placeholder="想了解的品类（如：生日蛋糕）"
                className="h-11 px-3 rounded-xl border border-gray-300 bg-white text-base outline-none focus:border-indigo-500"
              />
              <textarea
                value={fields.note} onChange={(e) => setFields({ ...fields, note: e.target.value })}
                placeholder="备注（可选）"
                rows={3}
                className="px-3 py-2.5 rounded-xl border border-gray-300 bg-white text-base outline-none focus:border-indigo-500 resize-none"
              />
              {err && <p className="text-xs text-red-500">{err}</p>}
              <button
                onClick={() => void submit()}
                disabled={busy || !fields.name.trim()}
                className="h-12 rounded-xl bg-[#4F46E5] text-white font-medium text-base disabled:opacity-40 active:scale-[0.98] transition-transform"
              >
                {busy ? '提交中…' : '提交'}
              </button>
              <p className="text-center text-xs text-gray-300">信息仅用于商家与你联系</p>
            </div>
          </>
        )}
      </div>
    </div>
  )
}

function PublicFormRoute() {
  const { formId } = useParams({ from: '/f/$formId' })
  return <PublicFormPage formId={formId} />
}

export const Route = createFileRoute('/f/$formId')({
  component: PublicFormRoute,
})
