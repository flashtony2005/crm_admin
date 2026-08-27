import { useEffect, useState } from 'react'
import { Button, Input } from '@heroui/react'
import { TextArea } from '@heroui/react/textarea'
import { publicComments, type Comment } from '../../api/cms'
import { useTranslation } from 'react-i18next'

/** 公开站文章评论区：列出已审核评论，访客可发表（后端可选审核）。 */
export function CommentsSection({ articleId }: { articleId: string }) {
  const { t } = useTranslation()
  const [list, setList] = useState<Comment[]>([])
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [content, setContent] = useState('')
  const [loading, setLoading] = useState(true)
  const [posting, setPosting] = useState(false)

  const load = () => {
    setLoading(true)
    publicComments.list(articleId).then(setList).finally(() => setLoading(false))
  }
  useEffect(load, [articleId])

  const post = async () => {
    if (!name.trim() || !content.trim()) return
    setPosting(true)
    try {
      await publicComments.create({ articleId, authorName: name, authorEmail: email, content })
      setName(''); setEmail(''); setContent('')
      load()
    } finally { setPosting(false) }
  }

  return (
    <section className="mt-10">
      <h2 className="text-lg font-semibold mb-4">{t('comments.title')}</h2>
      <div className="space-y-3 mb-6">
        {loading && <p className="text-sm text-os-text-muted">…</p>}
        {!loading && list.length === 0 && <p className="text-sm text-os-text-muted">暂无评论，来抢沙发。</p>}
        {list.map((c) => (
          <div key={c.id} className="border rounded-lg p-3">
            <p className="text-sm font-medium">{c.authorName}</p>
            <p className="text-sm text-os-text-secondary mt-1">{c.content}</p>
          </div>
        ))}
      </div>
      <div className="grid grid-cols-2 gap-2 mb-2">
        <Input placeholder={t('comments.placeholder')} value={name} onChange={(e) => setName(e.target.value)} />
        <Input placeholder="邮箱（选填）" value={email} onChange={(e) => setEmail(e.target.value)} />
      </div>
      <TextArea placeholder={t('comments.placeholder')} value={content} onChange={(e) => setContent(e.target.value)} rows={3} className="mb-2" />
      <Button size="sm" variant="primary" isDisabled={posting} onPress={() => void post()}>{t('comments.post')}</Button>
    </section>
  )
}
