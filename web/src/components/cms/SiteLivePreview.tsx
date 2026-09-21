/**
 * 真实主页可视化编辑（主端口活动模板）
 *
 * 读取 /api/public/templates 拿到「当前激活模板」与全部模板，iframe 直接加载
 * 该模板的预览地址（app 类走外部端口 http://host:port/；upload 类走后端
 * 同域 /t/<slug>/）。带 ?cmsEdit=1 让数据驱动站点启用编辑桥：
 *   - 鼠标移到可编辑元素上 → 站点内部画蓝色描边 + 标签
 *   - 点击元素 → postMessage({type:'cms:edit', ref}) 到这里 → 弹出对应编辑框
 *   - 保存后 postMessage({type:'cms:refresh'}) → 站点重新拉接口，预览即时更新
 *
 * 为什么不用图片/骨架图复刻：直接嵌真实页面才能做到「看到什么就改什么」。
 * 上传类模板若无编辑桥则仅为预览，其内容与其它模板同源，仍可在本页的
 * 区块 / 导航 / 置顶面板中维护。
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { Button } from '@heroui/react'
import { api } from '../../api/client'
import { ZoneEditDialog, parseEditRef, type EditTarget } from './ZoneEditDialog'

const HEIGHT = 720

interface Tpl {
  slug: string
  name: string
  kind: string
  port: string
  note: string
  active: boolean
  previewUrl: string
}

export function SiteLivePreview() {
  const iframeRef = useRef<HTMLIFrameElement | null>(null)
  const [ready, setReady] = useState(false)
  const [target, setTarget] = useState<EditTarget | null>(null)
  const [device, setDevice] = useState<'desktop' | 'mobile'>('desktop')
  const [nonce, setNonce] = useState(0)

  const [tpls, setTpls] = useState<Tpl[]>([])
  const [activeSlug, setActiveSlug] = useState('')
  const [selected, setSelected] = useState('')

  // 拉取模板清单 → 默认预览「主端口激活模板」
  useEffect(() => {
    let alive = true
    api<{ items: Tpl[]; active: string }>('/api/public/templates')
      .then((d) => {
        if (!alive) return
        setTpls(d.items || [])
        setActiveSlug(d.active || '')
        setSelected(d.active || (d.items?.[0]?.slug ?? ''))
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [])

  // 接收站点发来的编辑事件
  useEffect(() => {
    const onMsg = (e: MessageEvent) => {
      if (!iframeRef.current || e.source !== iframeRef.current.contentWindow) return
      const d = e.data
      if (!d || typeof d !== 'object') return
      if (d.type === 'cms:ready') setReady(true)
      if (d.type === 'cms:edit') {
        const t = parseEditRef(String(d.ref || ''), String(d.label || ''), String(d.text || ''))
        if (t) setTarget(t)
        else console.warn('[SiteLivePreview] 无法解析的 data-edit：', d.ref)
      }
    }
    window.addEventListener('message', onMsg)
    return () => window.removeEventListener('message', onMsg)
  }, [])

  const refresh = useCallback(() => {
    iframeRef.current?.contentWindow?.postMessage({ type: 'cms:refresh' }, '*')
  }, [])

  const reload = useCallback(() => {
    setReady(false)
    setNonce((n) => n + 1)
  }, [])

  const pick = (slug: string) => {
    setSelected(slug)
    setReady(false)
    setNonce((n) => n + 1)
  }

  const cur = tpls.find((x) => x.slug === selected)
  // 后端 previewUrl 可能是 /t/<slug>/（尾斜杠），axum 通配路由对尾斜杠结尾返回 404，故统一去掉
  const stripSlash = (u: string) => u.replace(/\/+$/, '')
  // app 类（coucouya / fastshot）带编辑桥 → 挂 cmsEdit；upload 类仅预览
  const src = cur
    ? cur.kind === 'app'
      ? `${stripSlash(cur.previewUrl)}?cmsEdit=1`
      : stripSlash(cur.previewUrl)
    : ''

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h3 className="text-base font-semibold">主页可视化编辑（主端口）</h3>
          <p className="mt-0.5 text-xs text-default-400">
            鼠标移到页面元素上会出现蓝色描边，点击即可编辑对应内容；切换模板查看不同风格。
          </p>
        </div>
        <div className="flex items-center gap-2">
          <span
            className={`h-1.5 w-1.5 rounded-full ${ready ? 'bg-success' : 'bg-default-300'}`}
            title={ready ? '已连接主页' : '等待主页响应'}
          />
          <span className="text-[11px] text-default-400">{ready ? '已连接' : '连接中…'}</span>
          <div className="ml-1 flex overflow-hidden rounded-lg border border-default-200">
            {(
              [
                ['desktop', '桌面'],
                ['mobile', '手机'],
              ] as const
            ).map(([k, label]) => (
              <button
                key={k}
                type="button"
                onClick={() => setDevice(k)}
                className={`px-2.5 py-1 text-[11px] transition-colors ${
                  device === k
                    ? 'bg-primary text-primary-foreground'
                    : 'bg-default-50 text-default-500 hover:bg-default-100'
                }`}
              >
                {label}
              </button>
            ))}
          </div>
          <Button variant="ghost" size="sm" onPress={refresh} isDisabled={!ready}>
            刷新数据
          </Button>
          <Button variant="ghost" size="sm" onPress={reload}>
            重载
          </Button>
        </div>
      </div>

      {/* 模板切换：默认主端口激活模板，可临时切到其它模板预览 */}
      {tpls.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[11px] text-default-400">预览模板：</span>
          {tpls.map((x) => {
            const on = x.slug === selected
            return (
              <button
                key={x.slug}
                type="button"
                onClick={() => pick(x.slug)}
                className={`rounded-full border px-2.5 py-1 text-[11px] transition-colors ${
                  on
                    ? 'border-primary bg-primary/10 text-primary'
                    : 'border-default-200 text-default-500 hover:bg-default-100'
                }`}
              >
                {x.name}
                {x.slug === activeSlug && <span className="ml-1 text-[10px] opacity-70">·主端口</span>}
              </button>
            )
          })}
        </div>
      )}

      <div className="rounded-xl border border-default-200 bg-default-100/60 p-2">
        <div
          className="mx-auto overflow-hidden rounded-lg bg-white shadow-sm transition-[width] duration-200"
          style={{ width: device === 'mobile' ? 390 : '100%', maxWidth: '100%' }}
        >
          {src ? (
            <iframe
              key={nonce}
              ref={iframeRef}
              src={src}
              title="主页预览"
              className="block w-full border-0"
              style={{ height: HEIGHT }}
            />
          ) : (
            <div className="flex h-[720px] items-center justify-center text-sm text-default-400">
              等待模板清单…
            </div>
          )}
        </div>
      </div>

      {!ready && src && (
        <p className="text-[11px] text-default-400">
          若长时间停留在「连接中」，请确认主页项目已启动（{cur?.previewUrl}）。数据驱动站点需包含编辑桥
          （src/cms/editBridge.ts）才能响应点击；上传类模板在此为只读预览，内容仍可在本页的区块 / 导航 / 置顶面板维护。
        </p>
      )}

      <ZoneEditDialog
        target={target}
        onClose={() => setTarget(null)}
        onSaved={refresh}
      />
    </div>
  )
}
