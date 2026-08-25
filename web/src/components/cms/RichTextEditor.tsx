import { useEffect, useRef } from 'react'

interface Props {
  value: string
  onChange: (html: string) => void
  placeholder?: string
  height?: number
}

/**
 * 把图片文件压缩为较小的 data URL，避免内联 base64 撑爆请求体（后端 Axum Json 默认 2MB 上限）。
 * - 长边超过 maxDim 时等比缩放到 maxDim 以内
 * - 照片/大图统一重编码为 JPEG(0.78)，体积通常降至原图的 1/5~1/10
 * - 仅当本身是 <300KB 的小 PNG 且无需缩放时，才保留 PNG（维持透明度）；其余一律 JPEG，确保体积小
 */
async function fileToCompressedDataUrl(
  file: File,
  maxDim = 1280,
  quality = 0.78,
): Promise<string> {
  const isPng = file.type === 'image/png'
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file)
    const img = new Image()
    img.onload = () => {
      URL.revokeObjectURL(url)
      let { width, height } = img
      const naturalW = img.width
      const naturalH = img.height
      // 小图且为 PNG：原样返回，避免无谓重编码（保透明度，且体积本就小）
      if (width <= maxDim && height <= maxDim && isPng && file.size < 300 * 1024) {
        const r = new FileReader()
        r.onload = () => resolve(r.result as string)
        r.onerror = () => reject(r.error)
        r.readAsDataURL(file)
        return
      }
      if (width > maxDim || height > maxDim) {
        const scale = Math.min(maxDim / width, maxDim / height)
        width = Math.round(width * scale)
        height = Math.round(height * scale)
      }
      const canvas = document.createElement('canvas')
      canvas.width = width
      canvas.height = height
      const ctx = canvas.getContext('2d')
      if (!ctx) {
        reject(new Error('canvas unavailable'))
        return
      }
      ctx.drawImage(img, 0, 0, width, height)
      // 一律输出 JPEG（含 PNG）：文章图片无需透明通道，JPEG 体积远小于 PNG
      resolve(canvas.toDataURL('image/jpeg', quality))
    }
    img.onerror = () => {
      URL.revokeObjectURL(url)
      reject(new Error('图片读取失败'))
    }
    img.src = url
  })
}

/**
 * 零依赖富文本编辑器（contentEditable + execCommand）。
 * - 工具栏：加粗 / 斜体 / 下划线 / 标题 / 正文 / 有序·无序列表 / 链接 / 图片
 * - 图片：以 FileReader 读为 data URL 后内联插入 <img>（内联 base64 方案，
 *   纯前端、无需后端上传端点）
 * 注：execCommand 虽被标记 deprecated，但在所有现代浏览器中仍可用，
 * 作为轻量 CMS 编辑器足够；后续可平滑替换为 TipTap。
 */
export function RichTextEditor({ value, onChange, placeholder, height = 320 }: Props) {
  const ref = useRef<HTMLDivElement>(null)
  const fileRef = useRef<HTMLInputElement>(null)

  // 外部值变化（编辑回填 / 切换记录）时同步到 DOM；
  // 仅在内容不一致时写入，避免正在输入时光标跳动。
  useEffect(() => {
    const el = ref.current
    if (el && value !== el.innerHTML) {
      el.innerHTML = value || ''
    }
  }, [value])

  const emit = () => {
    if (ref.current) onChange(ref.current.innerHTML)
  }

  const exec = (cmd: string, arg?: string) => {
    ref.current?.focus()
    document.execCommand(cmd, false, arg)
    emit()
  }

  const onBlock = (tag: string) => {
    ref.current?.focus()
    document.execCommand('formatBlock', false, tag)
    emit()
  }

  const onLink = () => {
    const url = window.prompt('输入链接地址（例如 https://example.com）', 'https://')
    if (!url) return
    ref.current?.focus()
    document.execCommand('createLink', false, url)
    emit()
  }

  const onPickImage = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    if (!file.type.startsWith('image/')) {
      window.alert('请选择图片文件')
      e.target.value = ''
      return
    }
    fileToCompressedDataUrl(file)
      .then((dataUrl) => {
        ref.current?.focus()
        document.execCommand(
          'insertHTML',
          false,
          `<img src="${dataUrl}" alt="${file.name}" style="max-width:100%;height:auto;border-radius:8px;margin:8px 0;display:block;" />`,
        )
        emit()
      })
      .catch(() => window.alert('图片处理失败，请换一张或重试'))
    e.target.value = ''
  }

  const btn =
    'px-2.5 h-8 rounded-md text-sm text-gray-600 hover:bg-indigo-50 hover:text-indigo-600 transition-colors select-none'

  return (
    <div className="border border-gray-200 rounded-lg overflow-hidden bg-white">
      <style>{`.rte-content:empty::before{content:attr(data-placeholder);color:#9ca3af;}`}</style>
      <div className="flex flex-wrap items-center gap-1 px-2 py-1.5 border-b border-gray-100 bg-gray-50">
        <button type="button" title="加粗" onClick={() => exec('bold')} className={`${btn} font-bold`}>B</button>
        <button type="button" title="斜体" onClick={() => exec('italic')} className={`${btn} italic`}>I</button>
        <button type="button" title="下划线" onClick={() => exec('underline')} className={`${btn} underline`}>U</button>
        <span className="w-px h-5 bg-gray-200 mx-1" />
        <button type="button" title="标题" onClick={() => onBlock('H2')} className={`${btn} font-semibold`}>H2</button>
        <button type="button" title="正文段落" onClick={() => onBlock('P')} className={btn}>¶</button>
        <span className="w-px h-5 bg-gray-200 mx-1" />
        <button type="button" title="无序列表" onClick={() => exec('insertUnorderedList')} className={btn}>• 列表</button>
        <button type="button" title="有序列表" onClick={() => exec('insertOrderedList')} className={btn}>1. 列表</button>
        <span className="w-px h-5 bg-gray-200 mx-1" />
        <button type="button" title="插入链接" onClick={onLink} className={btn}>🔗 链接</button>
        <button type="button" title="插入图片" onClick={() => fileRef.current?.click()} className={btn}>🖼 图片</button>
        <input ref={fileRef} type="file" accept="image/*" className="hidden" onChange={onPickImage} />
      </div>
      <div
        ref={ref}
        contentEditable
        suppressContentEditableWarning
        onInput={emit}
        data-placeholder={placeholder}
        style={{ minHeight: height }}
        className="rte-content px-3 py-2 text-sm leading-relaxed outline-none focus:ring-2 focus:ring-indigo-100 overflow-auto max-h-[60vh]"
      />
    </div>
  )
}
