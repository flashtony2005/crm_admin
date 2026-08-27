import { useState } from 'react'
import { Button, Input } from '@heroui/react'
import { newsletterApi } from '../../api/cms'
import { useTranslation } from 'react-i18next'

/** 公开站底部邮件订阅组件 */
export function NewsletterWidget() {
  const { t } = useTranslation()
  const [email, setEmail] = useState('')
  const [done, setDone] = useState(false)

  const submit = async () => {
    if (!email.includes('@')) return
    try {
      await newsletterApi.subscribe(email)
      setDone(true)
      setEmail('')
    } catch {
      /* 忽略错误展示，保持简洁 */
    }
  }

  return (
    <div className="rounded-2xl bg-os-surface/60 border p-6 my-8">
      <h3 className="font-semibold mb-1">{t('newsletter.title')}</h3>
      {done ? (
        <p className="text-sm text-green-600">已订阅，感谢关注！</p>
      ) : (
        <div className="flex gap-2 mt-3 max-w-md">
          <Input placeholder={t('newsletter.placeholder')} value={email} onChange={(e) => setEmail(e.target.value)} />
          <Button size="sm" variant="primary" onPress={() => void submit()}>{t('newsletter.subscribe')}</Button>
        </div>
      )}
    </div>
  )
}
