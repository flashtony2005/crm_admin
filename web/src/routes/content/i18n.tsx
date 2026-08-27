import { createFileRoute } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { i18nApi, type LocaleMessages } from '../../api/cms'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

function I18nPage() {
  const [zh, setZh] = useState<LocaleMessages>({})
  const [en, setEn] = useState<LocaleMessages>({})
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    Promise.all([i18nApi.messages('zh'), i18nApi.messages('en')])
      .then(([a, b]) => { setZh(a); setEn(b) })
      .finally(() => setLoading(false))
  }, [])

  const keys = Object.keys(zh)

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="多语言 i18n" desc="公开站界面文案翻译字典。内容侧多语言由文章的 locale 字段承载（公开列表支持 ?locale= 过滤）。" />
      <Auth perm={P.i18nView}>
        <div className="rounded-xl border bg-white overflow-hidden">
          <table className="w-full text-sm">
            <thead className="bg-gray-50 text-os-text-muted">
              <tr>
                <th className="text-left p-3 font-medium">键</th>
                <th className="text-left p-3 font-medium">中文 (zh)</th>
                <th className="text-left p-3 font-medium">English (en)</th>
              </tr>
            </thead>
            <tbody>
              {keys.map((k) => (
                <tr key={k} className="border-t">
                  <td className="p-3"><code className="text-xs">{k}</code></td>
                  <td className="p-3">{zh[k]}</td>
                  <td className="p-3">{en[k] ?? '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {loading && <p className="p-4 text-os-text-muted">加载中…</p>}
        </div>
      </Auth>
    </div>
  )
}

export const Route = createFileRoute('/content/i18n')({ component: I18nPage })
