'use client'
/**
 * Bottom Sheet —— 可拖拽的底部弹层
 *
 * 移植自 beui.dev/components/motion/bottom-sheet（MIT，作者 Saurabh）。
 * 适配本项目：framer-motion@12 + Tailwind v4 + 项目色板。
 *
 * 特性：
 *   - 可拖拽：拖顶部小横条上下拖动，快速下滑或拖过阈值即关闭
 *   - 多档吸附（snap points）：上滑可展开到更高的档位
 *   - 玻璃质感遮罩、打开时锁定背景滚动、Esc 关闭、Portal 到 body
 *
 * 与 beUI 原版的差异：
 *   - 用 `framer-motion` 替代 `motion/react`（本项目未装 motion 包）
 *   - 颜色令牌改为本项目 Tailwind 色板（原版用 shadcn 的 bg-card 等变量）
 *   - 去掉 PresenceGate（beUI 内部库，作用是退场瞬间释放交互）；
 *     本项目弹层退场仅 0.5s，重复触发关闭是幂等的，无副作用
 */

import {
  AnimatePresence,
  motion,
  type PanInfo,
  useDragControls,
  useReducedMotion,
} from 'framer-motion'
import { type ReactNode, useEffect, useId, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { cn } from '@/lib/utils'
import { EASE_DRAWER, TOUCH_GESTURE_CONTENT_CLASS } from './motion-tokens'

// 开合用长曲线完全阻尼的 tween，比 spring 更顺滑：不回弹、不抖动，只有一次干净的减速。
// 遮罩淡入用同一条曲线，保证面板和遮罩像一个整体在动。
const DRAWER = { duration: 0.5, ease: EASE_DRAWER } as const

export interface BottomSheetProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** 档位高度：0-1 为视口比例，或 'auto'（按内容自适应，上限 92vh）。第一项是默认值。 */
  snapPoints?: (number | 'auto')[]
  /** 初始档位索引。 */
  defaultSnap?: number
  title?: string
  description?: string
  children?: ReactNode
  className?: string
  /** 超过当前档位多少 px 视为下滑关闭。 */
  dismissThreshold?: number
}

export function BottomSheet({
  open,
  onOpenChange,
  snapPoints = [0.5, 0.92],
  defaultSnap = 0,
  title,
  description,
  children,
  className,
  dismissThreshold = 120,
}: BottomSheetProps) {
  const [snap, setSnap] = useState(defaultSnap)
  const [mounted, setMounted] = useState(false)
  const dragControls = useDragControls()
  const sheetRef = useRef<HTMLDivElement>(null)
  const reduce = useReducedMotion()
  const heightRef = useRef(0)
  const uid = useId()
  const titleId = `${uid}-title`
  const descriptionId = `${uid}-description`

  useEffect(() => {
    setMounted(true)
  }, [])

  useEffect(() => {
    if (open) setSnap(defaultSnap)
  }, [open, defaultSnap])

  // 打开时锁定背景滚动。仅 overflow:hidden 在 iOS Safari 上无效——
  // 弹层内部的滚动会穿透到页面，关闭后页面已经滚到别处了。
  // position:fixed 才是真正有效的锁，关闭时还原滚动位置。
  useEffect(() => {
    if (!open) return
    const body = document.body
    const scrollY = window.scrollY
    const prev = {
      position: body.style.position,
      top: body.style.top,
      left: body.style.left,
      right: body.style.right,
      overflow: body.style.overflow,
    }
    body.style.position = 'fixed'
    body.style.top = `-${scrollY}px`
    body.style.left = '0'
    body.style.right = '0'
    body.style.overflow = 'hidden'

    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        onOpenChange(false)
      }
    }
    window.addEventListener('keydown', onKey)

    return () => {
      window.removeEventListener('keydown', onKey)
      body.style.position = prev.position
      body.style.top = prev.top
      body.style.left = prev.left
      body.style.right = prev.right
      body.style.overflow = prev.overflow
      window.scrollTo(0, scrollY)
    }
  }, [open, onOpenChange])

  const onDragEnd = (_: unknown, info: PanInfo) => {
    const velocity = info.velocity.y
    const offset = info.offset.y

    // 用力下甩或大幅下拉 → 关闭。
    if (velocity > 600 || offset > dismissThreshold) {
      const smaller = snapPoints.map((_, i) => i).filter((i) => i < snap)
      if (smaller.length && velocity < 800 && offset < dismissThreshold * 1.6) {
        setSnap(smaller[smaller.length - 1])
      } else {
        onOpenChange(false)
      }
      return
    }

    // 用力上甩 → 升到下一档。
    if (velocity < -500) {
      setSnap((current) => Math.min(snapPoints.length - 1, current + 1))
      return
    }

    // 其余情况按位移吸附到相邻档位。
    setSnap((current) => {
      if (offset > 80 && current > 0) return current - 1
      if (offset < -80 && current < snapPoints.length - 1) return current + 1
      return current
    })
  }

  const snapValue = snapPoints[snap]
  const heightStyle =
    snapValue === 'auto'
      ? { maxHeight: '92vh' }
      : { height: `${snapValue * 100}vh` }

  // Portal 到 <body>：祖先元素上只要有 backdrop-filter 或 transform，
  // 就会成为 fixed 子元素的包含块，导致弹层相对该祖先而不是视口定位。
  if (!mounted) return null

  return createPortal(
    <AnimatePresence>
      {open ? (
        <motion.button
          key="backdrop"
          type="button"
          aria-label="关闭弹层"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={DRAWER}
          onClick={() => onOpenChange(false)}
          // 轻微模糊 + 较高不透明度：backdrop-blur 很吃 GPU，弹层每拖一帧都要重绘，
          // 小半径 + 更实的底色能保住玻璃感又不掉帧。
          className="pointer-events-auto fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
        />
      ) : null}
      {open ? (
        <motion.div
          key="sheet"
          ref={sheetRef}
          drag="y"
          dragControls={dragControls}
          // dragListener=false：只允许拖顶部小横条，内容区保持可选中。
          dragListener={false}
          dragConstraints={{ top: 0, bottom: 0 }}
          dragElastic={{ top: 0.02, bottom: 0.4 }}
          dragMomentum={false}
          onDragEnd={onDragEnd}
          initial={reduce ? { y: 0, opacity: 0 } : { y: '100%' }}
          animate={reduce ? { y: 0, opacity: 1 } : { y: 0 }}
          exit={reduce ? { y: 0, opacity: 0 } : { y: '100%' }}
          transition={reduce ? { duration: 0.18, ease: EASE_DRAWER } : DRAWER}
          onAnimationComplete={() => {
            if (sheetRef.current) heightRef.current = sheetRef.current.offsetHeight
          }}
          style={heightStyle}
          className={cn(
            'pointer-events-auto fixed bottom-0 left-0 right-0 z-50 mx-auto flex max-w-md flex-col overflow-hidden rounded-t-3xl will-change-transform',
            'border border-black/[0.06] bg-white shadow-2xl',
            className,
          )}
          role="dialog"
          aria-modal="true"
          aria-labelledby={title ? titleId : undefined}
          aria-describedby={description ? descriptionId : undefined}
          aria-label={title ? undefined : '底部弹层'}
        >
          {/* 拖拽手柄区 */}
          <div className="flex flex-col items-center px-4 pb-2 pt-3">
            <div
              onPointerDown={(event) => dragControls.start(event)}
              // touch-none：慢速拖拽不能交给 iOS 的长按菜单，否则弹层会卡在半路。
              className={cn(
                'flex cursor-grab touch-none items-center justify-center py-1 active:cursor-grabbing',
                TOUCH_GESTURE_CONTENT_CLASS,
              )}
            >
              <div className="h-1.5 w-10 rounded-full bg-black/20" />
            </div>
            {title || description ? (
              <div className="mt-2 w-full">
                {title ? (
                  <h2 id={titleId} className="text-base font-semibold text-gray-900">
                    {title}
                  </h2>
                ) : null}
                {description ? (
                  <p id={descriptionId} className="mt-0.5 text-sm text-gray-500">
                    {description}
                  </p>
                ) : null}
              </div>
            ) : null}
          </div>
          {/* overscroll-contain 阻止内部滚动穿透到页面 */}
          <div className="flex-1 overflow-y-auto overscroll-contain px-4 pb-6">
            {children}
          </div>
        </motion.div>
      ) : null}
    </AnimatePresence>,
    document.body,
  )
}
