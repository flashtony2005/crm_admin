export function cn(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(' ')
}

/**
 * 计算分页页码序列（含省略号标记）。
 * 例：paginationRange(5, 20, 1) -> [1, 'left', 4, 5, 6, 'right', 20]
 * 返回项：number 表示页码，'left'/'right' 表示左侧/右侧省略号。
 */
export function paginationRange(
  current: number,
  total: number,
  sibling = 1,
): Array<number | 'left' | 'right'> {
  const totalNum = Math.max(1, total)
  const currentNum = Math.min(Math.max(1, current), totalNum)
  const maxVisible = sibling * 2 + 5 // 首页 + 尾页 + 当前 + 2*sibling + 2 省略号占位

  if (totalNum <= maxVisible) {
    return Array.from({ length: totalNum }, (_, i) => i + 1)
  }

  const leftSibling = Math.max(currentNum - sibling, 1)
  const rightSibling = Math.min(currentNum + sibling, totalNum)
  const showLeftEllipsis = leftSibling > 2
  const showRightEllipsis = rightSibling < totalNum - 1

  const range: Array<number | 'left' | 'right'> = [1]
  if (showLeftEllipsis) {
    range.push('left')
  } else {
    for (let i = 2; i < leftSibling; i++) range.push(i)
  }
  // Page 1 is already range[0]; the middle window starts at >= 2 to avoid
  // re-adding it when current lands on page 1 (leftSibling === 1).
  for (let i = Math.max(leftSibling, 2); i <= rightSibling; i++) range.push(i)
  if (showRightEllipsis) {
    range.push('right')
  } else {
    for (let i = rightSibling + 1; i < totalNum; i++) range.push(i)
  }
  range.push(totalNum)
  return range
}
