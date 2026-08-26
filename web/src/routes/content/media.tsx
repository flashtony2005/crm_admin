import { createFileRoute } from '@tanstack/react-router'
import { useRef, useState } from 'react'
import { Button } from '@heroui/react'

import { mediaApi, uploadFile, type MediaItem } from '../../api/cms'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'
import { fmtDate, fmtSize } from '../../components/cms/format'

const TYPE_ICON: Record<MediaItem['type'], string> = {
  image: '🖼️',
  video: '🎬',
  file: '📎',
}

function MediaPage() {
  const t = useCmsCollection(mediaApi, ['cms-media'], { searchFields: ['name'] })
  const fileRef = useRef<HTMLInputElement>(null)
  const [busy, setBusy] = useState(false)

  /** 真实上传：选择文件 → POST /api/upload → 记录返回的真实 URL */
  const handleFiles = async (files: FileList | null) => {
    if (!files || files.length === 0) return
    setBusy(true)
    try {
      for (const f of Array.from(files)) {
        const uploaded = await uploadFile(f)
        await t.create.mutateAsync({
          name: uploaded.name || f.name,
          type: uploaded.type as MediaItem['type'],
          sizeKb: uploaded.sizeKb,
          url: uploaded.url,
        })
      }
    } catch (e) {
      window.alert(e instanceof Error ? e.message : '上传失败，请重试')
    } finally {
      setBusy(false)
      if (fileRef.current) fileRef.current.value = ''
    }
  }

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="Media" desc="图片、视频与素材库，文章和产品都可以引用。" />
      <CmsToolbar searchPlaceholder="搜索素材…" searchValue={t.search} onSearchChange={t.setSearch}>
        <input
          ref={fileRef}
          type="file"
          multiple
          accept="image/*,video/*,.pdf,.doc,.docx"
          className="hidden"
          onChange={(e) => handleFiles(e.target.files)}
        />
        <Auth perm={P.contentMediaUpload}>
          <Button variant="primary" size="sm" isDisabled={busy} onPress={() => fileRef.current?.click()}>
            {busy ? '上传中…' : '⬆ 上传素材'}
          </Button>
        </Auth>
      </CmsToolbar>

      {t.isLoading ? (
        <p className="text-sm text-os-text-muted py-16 text-center">加载中…</p>
      ) : t.paged.length === 0 ? (
        <div className="flex flex-col items-center justify-center h-64 text-os-text-muted">
          <div className="text-5xl mb-3 opacity-60">🖼️</div>
          <p>还没有素材</p>
          <p className="text-sm mt-1">点击「上传素材」，或让 AI 从文章里自动配图</p>
        </div>
      ) : (
        <div className="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-4 gap-3">
          {t.paged.map((m) => (
            <figure
              key={m.id}
              className="group relative rounded-xl border border-os-border bg-white overflow-hidden shadow-sm hover:shadow-md transition-all"
            >
              {/* 缩略区：图片渲染真实上传图，其它类型用图标占位 */}
              {m.type === 'image' && m.url && m.url !== '#' ? (
                <img src={m.url} alt={m.name} className="h-28 w-full object-cover" loading="lazy" />
              ) : (
                <div className="h-28 flex items-center justify-center bg-gradient-to-br from-gray-50 to-gray-100 text-4xl">
                  {TYPE_ICON[m.type] ?? '📎'}
                </div>
              )}
              <figcaption className="px-3 py-2.5">
                <p className="text-sm font-medium text-os-text-primary truncate">{m.name}</p>
                <p className="text-xs text-os-text-muted mt-0.5">
                  {fmtSize(m.sizeKb)} · {fmtDate(m.updatedAt)}
                </p>
              </figcaption>
              <Auth perm={P.contentMediaDelete}>
                <button
                  type="button"
                  onClick={() => window.confirm(`确定删除「${m.name}」吗？`) && t.remove.mutateAsync(m.id)}
                  aria-label={`删除 ${m.name}`}
                  className="absolute top-2 right-2 w-7 h-7 rounded-lg bg-white/90 border border-os-border-light text-red-500 opacity-0 group-hover:opacity-100 transition-opacity"
                >
                  ✕
                </button>
              </Auth>
            </figure>
          ))}
        </div>
      )}
    </div>
  )
}

export const Route = createFileRoute('/content/media')({
  component: MediaPage,
})
