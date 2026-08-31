'use client'
/**
 * Swipeable List —— 可滑动露出操作的列表（移动端经典交互）
 *
 * 移植自 beui.dev/components/blocks/swipeable-list（MIT，作者 Saurabh）。
 * 适配本项目：framer-motion@12 + Tailwind v4 + 项目色板。
 *
 * 特性：
 *   - 行可左右拖动，露出左/右两侧的操作按钮（编辑、删除等）
 *   - 基于位移与速度的释放判定：轻甩即开、短距回弹，接近原生手感
 *   - 同一时刻只允许一行处于展开态；拖其他行会自动收起前一行
 *   - 支持 onTap（点击行）——拖动时自动不触发点击
 *   - 尊重系统的「减弱动画」设置
 *
 * 与 beUI 原版的差异：
 *   - 用 `framer-motion` 替代 `motion/react`
 *   - 颜色令牌改为本项目 Tailwind 色板
 *   - 新增 `onTap`：原版只有操作按钮，本项目需要「点行看详情」
 */

import { animate, motion, useMotionValue, useReducedMotion, type PanInfo } from 'framer-motion'
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { cn } from '@/lib/utils'
import { TOUCH_GESTURE_CONTENT_CLASS } from './motion-tokens'

export type SwipeSide = 'left' | 'right'

export type SwipeableListValue = { id: string; side: SwipeSide }

export type SwipeActionTone = 'neutral' | 'primary' | 'success' | 'warning' | 'danger'

export type SwipeAction = {
  id: string
  label: ReactNode
  icon: ReactNode
  tone?: SwipeActionTone
  disabled?: boolean
  onClick?: (item: SwipeableListItem) => void
}

export type SwipeableListItem = {
  id: string
  title?: ReactNode
  description?: ReactNode
  meta?: ReactNode
  leading?: ReactNode
  content?: ReactNode
  leftActions?: SwipeAction[]
  rightActions?: SwipeAction[]
  disabled?: boolean
}

export type SwipeableListClassNames = {
  root?: string
  item?: string
  rail?: string
  action?: string
  surface?: string
  leading?: string
  content?: string
  title?: string
  description?: string
  meta?: string
}

export interface SwipeableListProps {
  items: SwipeableListItem[]
  value?: SwipeableListValue | null
  defaultValue?: SwipeableListValue | null
  onValueChange?: (value: SwipeableListValue | null) => void
  onAction?: (payload: { item: SwipeableListItem; action: SwipeAction; side: SwipeSide }) => void
  /** 点击行（拖动时不会触发）。 */
  onTap?: (item: SwipeableListItem) => void
  /** 单个操作按钮宽度 px。 */
  actionWidth?: number
  /** 展开所需的最小拖动距离 px。 */
  revealThreshold?: number
  closeOnAction?: boolean
  className?: string
  classNames?: SwipeableListClassNames
  renderItem?: (item: SwipeableListItem) => ReactNode
}

// 基于距离的释放弹簧：短距回弹和完整展开的手感一致，更接近原生列表。
const ROW_SETTLE = {
  type: 'spring',
  stiffness: 560,
  damping: 48,
  mass: 0.82,
  restDelta: 0.5,
  restSpeed: 8,
} as const

const OPEN_DISTANCE_RATIO = 0.46
const CLOSE_DISTANCE_RATIO = 0.72
const OPEN_VELOCITY = 720
const CLOSE_VELOCITY = 320
const FLING_DISTANCE = 14
const RELEASE_VELOCITY_LIMIT = 1500

const ACTION_TONE_CLASS: Record<SwipeActionTone, string> = {
  neutral: 'text-gray-400 group-hover:text-gray-600',
  primary: 'text-[#6C4DF6]',
  success: 'text-emerald-600',
  warning: 'text-amber-600',
  danger: 'text-red-500',
}

function useControllableSwipeValue({
  value,
  defaultValue,
  onValueChange,
}: {
  value?: SwipeableListValue | null
  defaultValue?: SwipeableListValue | null
  onValueChange?: (value: SwipeableListValue | null) => void
}) {
  const [internalValue, setInternalValue] = useState(defaultValue ?? null)
  const isControlled = value !== undefined
  const currentValue = value ?? internalValue

  const setValue = useCallback(
    (next: SwipeableListValue | null) => {
      if (!isControlled) setInternalValue(next)
      onValueChange?.(next)
    },
    [isControlled, onValueChange],
  )

  return [currentValue, setValue] as const
}

function isActionableSide(value: number, sideWidth: number) {
  return sideWidth > 0 && Math.abs(value) > 0
}

function clampReleaseVelocity(velocity: number) {
  return Math.max(-RELEASE_VELOCITY_LIMIT, Math.min(RELEASE_VELOCITY_LIMIT, velocity))
}

function SwipeActionButton({
  action,
  actionWidth,
  side,
  focusable,
  onAction,
  className,
}: {
  action: SwipeAction
  actionWidth: number
  side: SwipeSide
  focusable: boolean
  onAction: (action: SwipeAction, side: SwipeSide) => void
  className?: string
}) {
  return (
    <button
      type="button"
      disabled={action.disabled}
      tabIndex={focusable ? 0 : -1}
      aria-label={typeof action.label === 'string' ? action.label : undefined}
      onClick={() => onAction(action, side)}
      className={cn(
        'group flex h-full shrink-0 items-center justify-center outline-none',
        'focus-visible:ring-2 focus-visible:ring-[#7C5CFF]',
        'disabled:pointer-events-none disabled:opacity-50',
        className,
      )}
      style={{ width: actionWidth }}
    >
      <span
        className={cn(
          'grid h-9 w-9 place-items-center rounded-full transition-[background-color,color,transform] duration-150',
          'group-hover:bg-white group-active:scale-95',
          ACTION_TONE_CLASS[action.tone ?? 'neutral'],
        )}
      >
        {action.icon}
      </span>
      <span className="sr-only">{action.label}</span>
    </button>
  )
}

function SwipeableListRow({
  item,
  actionWidth,
  revealThreshold,
  openValue,
  setOpenValue,
  closeOnAction,
  onAction,
  onTap,
  classNames,
  renderItem,
}: {
  item: SwipeableListItem
  actionWidth: number
  revealThreshold: number
  openValue: SwipeableListValue | null
  setOpenValue: (value: SwipeableListValue | null) => void
  closeOnAction: boolean
  onAction?: SwipeableListProps['onAction']
  onTap?: (item: SwipeableListItem) => void
  classNames?: SwipeableListClassNames
  renderItem?: (item: SwipeableListItem) => ReactNode
}) {
  const reduce = useReducedMotion()
  const x = useMotionValue(0)
  const animationRef = useRef<{ stop: () => void } | null>(null)
  const commandedTargetRef = useRef(0)
  const leftActions = item.leftActions ?? []
  const rightActions = item.rightActions ?? []
  const leftWidth = leftActions.length * actionWidth
  const rightWidth = rightActions.length * actionWidth
  const openSide = openValue?.id === item.id ? openValue.side : null
  const targetX = openSide === 'left' ? leftWidth : openSide === 'right' ? -rightWidth : 0

  const settleX = useCallback(
    (nextX: number, velocity = 0) => {
      commandedTargetRef.current = nextX
      animationRef.current?.stop()

      if (reduce) {
        x.set(nextX)
        return
      }

      animationRef.current = animate(x, nextX, {
        ...ROW_SETTLE,
        velocity: clampReleaseVelocity(velocity),
        onComplete: () => x.set(nextX),
      })
    },
    [reduce, x],
  )

  useEffect(() => () => animationRef.current?.stop(), [])

  useEffect(() => {
    if (commandedTargetRef.current === targetX) return
    settleX(targetX)
  }, [settleX, targetX])

  const getTargetX = useCallback(
    (side: SwipeSide | null) =>
      side === 'left' ? leftWidth : side === 'right' ? -rightWidth : 0,
    [leftWidth, rightWidth],
  )

  const snapTo = useCallback(
    (side: SwipeSide | null, velocity = 0) => {
      setOpenValue(side ? { id: item.id, side } : null)
      settleX(getTargetX(side), velocity)
    },
    [getTargetX, item.id, setOpenValue, settleX],
  )

  const onDragStart = useCallback(() => {
    animationRef.current?.stop()
    // 拖 A 行时自动收起已展开的 B 行，保证同时只有一行展开。
    if (openValue && openValue.id !== item.id) setOpenValue(null)
  }, [item.id, openValue, setOpenValue])

  const onDragEnd = useCallback(
    (_: PointerEvent, info: PanInfo) => {
      const velocity = info.velocity.x
      const latest = x.get()
      const leftOpenThreshold = Math.max(revealThreshold, leftWidth * OPEN_DISTANCE_RATIO)
      const rightOpenThreshold = Math.max(revealThreshold, rightWidth * OPEN_DISTANCE_RATIO)

      if (openSide === 'left') {
        if (latest < leftWidth * CLOSE_DISTANCE_RATIO || velocity < -CLOSE_VELOCITY) {
          snapTo(null, velocity)
          return
        }
        snapTo('left', velocity)
        return
      }

      if (openSide === 'right') {
        if (
          Math.abs(latest) < rightWidth * CLOSE_DISTANCE_RATIO ||
          velocity > CLOSE_VELOCITY
        ) {
          snapTo(null, velocity)
          return
        }
        snapTo('right', velocity)
        return
      }

      if (
        isActionableSide(latest, leftWidth) &&
        (latest > leftOpenThreshold || (velocity > OPEN_VELOCITY && latest > FLING_DISTANCE))
      ) {
        snapTo('left', velocity)
        return
      }

      if (
        isActionableSide(latest, rightWidth) &&
        (latest < -rightOpenThreshold ||
          (velocity < -OPEN_VELOCITY && latest < -FLING_DISTANCE))
      ) {
        snapTo('right', velocity)
        return
      }

      snapTo(null, velocity)
    },
    [leftWidth, openSide, revealThreshold, rightWidth, snapTo, x],
  )

  const handleAction = useCallback(
    (action: SwipeAction, side: SwipeSide) => {
      action.onClick?.(item)
      onAction?.({ item, action, side })
      if (closeOnAction) snapTo(null)
    },
    [closeOnAction, item, onAction, snapTo],
  )

  const defaultContent = (
    <div className="flex min-w-0 items-center gap-3">
      {item.leading ? (
        <div className={cn('shrink-0', classNames?.leading)}>{item.leading}</div>
      ) : null}
      <div className={cn('min-w-0 flex-1', classNames?.content)}>
        {item.title ? (
          <div className={cn('truncate text-sm font-medium text-gray-900', classNames?.title)}>
            {item.title}
          </div>
        ) : null}
        {item.description ? (
          <div className={cn('mt-0.5 truncate text-xs text-gray-400', classNames?.description)}>
            {item.description}
          </div>
        ) : null}
      </div>
      {item.meta ? (
        <div
          className={cn('shrink-0 text-xs font-medium text-gray-400', classNames?.meta)}
        >
          {item.meta}
        </div>
      ) : null}
    </div>
  )

  return (
    <div
      className={cn(
        'relative isolate overflow-hidden rounded-3xl bg-[#F1F2F7]',
        item.disabled && 'opacity-60',
        classNames?.item,
      )}
    >
      {/* 操作轨道：位于行下方，未展开时用 inert 移出可交互树 */}
      <div
        aria-hidden={!openSide}
        inert={!openSide}
        className={cn('absolute inset-0 z-0 flex overflow-hidden rounded-3xl', classNames?.rail)}
      >
        <div className="flex h-full overflow-hidden rounded-l-3xl">
          {leftActions.map((action) => (
            <SwipeActionButton
              key={action.id}
              action={action}
              actionWidth={actionWidth}
              className={classNames?.action}
              focusable={openSide === 'left'}
              onAction={handleAction}
              side="left"
            />
          ))}
        </div>
        <div className="ml-auto flex h-full overflow-hidden rounded-r-3xl">
          {rightActions.map((action) => (
            <SwipeActionButton
              key={action.id}
              action={action}
              actionWidth={actionWidth}
              className={classNames?.action}
              focusable={openSide === 'right'}
              onAction={handleAction}
              side="right"
            />
          ))}
        </div>
      </div>

      <motion.div
        drag={item.disabled ? false : 'x'}
        dragConstraints={{ left: -rightWidth, right: leftWidth }}
        dragElastic={0.04}
        dragMomentum={false}
        onDragStart={onDragStart}
        onDragEnd={onDragEnd}
        onTap={() => onTap?.(item)}
        style={{ x }}
        className={cn(
          // touch-pan-y 而非 touch-none：行接管横向滑动，纵向仍可滚页面。
          'relative z-10 min-h-[72px] cursor-grab touch-pan-y rounded-3xl border border-black/[0.04] bg-white px-4 py-3 shadow-sm active:cursor-grabbing',
          TOUCH_GESTURE_CONTENT_CLASS,
          classNames?.surface,
        )}
      >
        {renderItem ? renderItem(item) : item.content ?? defaultContent}
      </motion.div>
    </div>
  )
}

export function SwipeableList({
  items,
  value,
  defaultValue = null,
  onValueChange,
  onAction,
  onTap,
  actionWidth = 56,
  revealThreshold = 34,
  closeOnAction = true,
  className,
  classNames,
  renderItem,
}: SwipeableListProps) {
  const [openValue, setOpenValue] = useControllableSwipeValue({ value, defaultValue, onValueChange })

  return (
    <div className={cn('flex w-full flex-col gap-2', className, classNames?.root)}>
      {items.map((item) => (
        <SwipeableListRow
          key={item.id}
          item={item}
          actionWidth={actionWidth}
          revealThreshold={revealThreshold}
          openValue={openValue}
          setOpenValue={setOpenValue}
          closeOnAction={closeOnAction}
          onAction={onAction}
          onTap={onTap}
          classNames={classNames}
          renderItem={renderItem}
        />
      ))}
    </div>
  )
}
