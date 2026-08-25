import React, { useRef, useState, useCallback, useEffect } from 'react'
import { Check, ArrowRight } from 'lucide-react'

interface DragVerifyProps {
  onSuccess: () => void
  onReset?: () => void
  width?: number
  height?: number
  text?: string
  successText?: string
  className?: string
}

export const DragVerify: React.FC<DragVerifyProps> = ({
  onSuccess,
  onReset,
  width = 320,
  height = 44,
  text = '请按住滑块拖动到最右边',
  successText = '验证通过',
  className = '',
}) => {
  const [position, setPosition] = useState(0)
  const [isDragging, setIsDragging] = useState(false)
  const [isSuccess, setIsSuccess] = useState(false)
  const trackRef = useRef<HTMLDivElement>(null)
  const handleSize = height + 6

  const maxPosition = width - handleSize

  const handleStart = useCallback(
    (_clientX: number) => {
      if (isSuccess) return
      setIsDragging(true)
    },
    [isSuccess],
  )

  const handleMove = useCallback(
    (clientX: number) => {
      if (!isDragging || isSuccess || !trackRef.current) return
      const rect = trackRef.current.getBoundingClientRect()
      let newPos = clientX - rect.left - handleSize / 2
      newPos = Math.max(0, Math.min(newPos, maxPosition))
      setPosition(newPos)

      if (newPos >= maxPosition - 3) {
        setIsDragging(false)
        setIsSuccess(true)
        setPosition(maxPosition)
        onSuccess()
      }
    },
    [isDragging, isSuccess, maxPosition, onSuccess, handleSize],
  )

  const handleEnd = useCallback(() => {
    if (!isSuccess) {
      setPosition(0)
    }
    setIsDragging(false)
  }, [isSuccess])

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault()
      handleStart(e.clientX)
    },
    [handleStart],
  )

  // 重置动画：非拖拽且非成功时回到 0
  useEffect(() => {
    if (!isDragging && !isSuccess) {
      setPosition(0)
      onReset?.()
    }
  }, [isDragging, isSuccess, onReset])

  // 全局鼠标事件
  useEffect(() => {
    if (!isDragging) return
    const onMouseMove = (e: MouseEvent) => handleMove(e.clientX)
    const onMouseUp = () => handleEnd()
    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)
    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
    }
  }, [isDragging, handleMove, handleEnd])

  // 全局触摸事件
  const onTouchStart = useCallback(
    (e: React.TouchEvent) => {
      handleStart(e.touches[0].clientX)
    },
    [handleStart],
  )

  useEffect(() => {
    if (!isDragging) return
    const onTouchMove = (e: TouchEvent) => {
      e.preventDefault()
      handleMove(e.touches[0].clientX)
    }
    const onTouchEnd = () => handleEnd()
    window.addEventListener('touchmove', onTouchMove, { passive: false })
    window.addEventListener('touchend', onTouchEnd)
    return () => {
      window.removeEventListener('touchmove', onTouchMove)
      window.removeEventListener('touchend', onTouchEnd)
    }
  }, [isDragging, handleMove, handleEnd])

  const progress = maxPosition > 0 ? (position / maxPosition) * 100 : 0

  return (
    <div className={`relative inline-flex flex-col items-center ${className}`} style={{ width }}>
      {/* 轨道 + 进度 + 文字 */}
      <div
        ref={trackRef}
        className="relative select-none rounded-full overflow-hidden"
        style={{ width, height }}
      >
        {/* 轨道底色 */}
        <div
          className="absolute inset-0 rounded-full transition-colors duration-300"
          style={{
            background: isSuccess
              ? 'linear-gradient(to right, #22c55e, #16a34a)'
              : '#e5e7eb',
          }}
        />
        {/* 进度条 */}
        <div
          className="absolute inset-y-0 left-0 rounded-full transition-all duration-200"
          style={{
            width: isSuccess ? '100%' : `${progress}%`,
            background: isSuccess
              ? 'linear-gradient(to right, #22c55e, #16a34a)'
              : 'linear-gradient(to right, #6366f1, #8b5cf6)',
          }}
        />
        {/* 提示文字 */}
        {!isSuccess && position === 0 && (
          <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <span className="text-sm text-gray-400 select-none">{text}</span>
          </div>
        )}
        {/* 成功文字 */}
        {isSuccess && (
          <div className="absolute inset-0 flex items-center justify-center gap-1.5">
            <Check size={16} className="text-white" />
            <span className="text-sm font-medium text-white select-none">
              {successText}
            </span>
          </div>
        )}
      </div>

      {/* 拖动手柄：绝对定位在轨道上方 */}
      <div
        className="absolute flex items-center justify-center rounded-full cursor-grab active:cursor-grabbing shadow-md z-10 transition-shadow duration-200"
        style={{
          width: handleSize,
          height: handleSize,
          left: position,
          top: (height - handleSize) / 2,
          background: isSuccess
            ? '#22c55e'
            : isDragging
              ? '#6366f1'
              : '#ffffff',
          border: isSuccess
            ? '2px solid #16a34a'
            : isDragging
              ? '2px solid #4f46e5'
              : '2px solid #d1d5db',
        }}
        onMouseDown={onMouseDown}
        onTouchStart={onTouchStart}
      >
        {isSuccess ? (
          <Check size={20} className="text-white" />
        ) : (
          <ArrowRight
            size={20}
            className={isDragging ? 'text-white' : 'text-gray-400'}
          />
        )}
      </div>
    </div>
  )
}
