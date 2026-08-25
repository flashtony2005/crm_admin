import { useMemo, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { CrudService } from '../../api/cms/store'

/**
 * CMS 列表页通用数据层（抽象基座，供所有集合页面复用）。
 *
 * 职责：
 * - 列表加载（TanStack Query）+ 增删改 mutation + 自动失效缓存；
 * - 客户端即时搜索（小数据集直接前端过滤）+ 分页；
 * - Phase 1 数据源为本地适配器；切换真实后端时本 hook 无需改动。
 */
export function useCmsCollection<T extends { id: string; updatedAt?: string }>(
  api: CrudService<T>,
  queryKey: readonly string[],
  opts?: {
    /** 参与搜索匹配的字符串字段 */
    searchFields?: (keyof T & string)[]
    pageSize?: number
    /** 自定义过滤（在 searchFields 之后追加），返回 false 表示剔除 */
    extraFilter?: (row: T, query: string) => boolean
  },
) {
  const pageSize = opts?.pageSize ?? 10
  const searchFields = opts?.searchFields ?? []
  const qc = useQueryClient()

  // extraFilter 通过 ref 读取，避免调用方传内联函数导致 memo 失效
  const extraFilterRef = useRef(opts?.extraFilter)
  extraFilterRef.current = opts?.extraFilter

  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)

  const listQuery = useQuery({
    queryKey: [...queryKey, 'list'],
    queryFn: () => api.list(),
  })
  const items = useMemo(() => listQuery.data ?? [], [listQuery.data])

  const searchKey = searchFields.join('\u0000')
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    if (!q) return items
    return items.filter((row) => {
      const hit = searchFields.some((f) =>
        String(row[f] ?? '').toLowerCase().includes(q),
      )
      if (!hit) return false
      return extraFilterRef.current ? extraFilterRef.current(row, q) : true
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps -- searchFields 以 searchKey（拼接串）参与依赖
  }, [items, search, searchKey])

  const pageCount = Math.max(1, Math.ceil(filtered.length / pageSize))
  const safePage = Math.min(page, pageCount)
  const paged = useMemo(
    () => filtered.slice((safePage - 1) * pageSize, safePage * pageSize),
    [filtered, safePage, pageSize],
  )

  const invalidate = () => qc.invalidateQueries({ queryKey })

  const create = useMutation({
    mutationFn: (input: Parameters<typeof api.create>[0]) => api.create(input),
    onSuccess: invalidate,
  })
  const update = useMutation({
    mutationFn: ({ id, patch }: { id: string; patch: Partial<T> }) => api.update(id, patch),
    onSuccess: invalidate,
  })
  const remove = useMutation({
    mutationFn: (id: string) => api.remove(id),
    onSuccess: invalidate,
  })

  return {
    // 数据态
    items,
    filtered,
    paged,
    total: filtered.length,
    isLoading: listQuery.isLoading,
    isError: listQuery.isError,
    refetch: listQuery.refetch,
    // 搜索 / 分页
    search,
    setSearch: (v: string) => {
      setSearch(v)
      setPage(1)
    },
    page: safePage,
    pageCount,
    setPage,
    pageSize,
    // CRUD
    create,
    update,
    remove,
    isMutating: create.isPending || update.isPending || remove.isPending,
  }
}
