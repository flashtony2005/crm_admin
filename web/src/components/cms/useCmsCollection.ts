import { useEffect, useMemo, useRef, useState } from 'react'
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
    /** 服务端分页（大表启用）：按页拉取 + 服务端 total；搜索仅作用于当前页 */
    serverPaged?: boolean
    /**
     * 服务端等值过滤（白名单列）：如 { firstTouchArticleId: 'xxx' }。
     * 值变化会重新请求；空串视为不过滤。
     */
    filters?: Record<string, string>
    /** 自定义过滤（在 searchFields 之后追加），返回 false 表示剔除 */
    extraFilter?: (row: T, query: string) => boolean
  },
) {
  const pageSize = opts?.pageSize ?? 10
  const serverPaged = opts?.serverPaged ?? false
  const searchFields = opts?.searchFields ?? []
  const qc = useQueryClient()

  // extraFilter 通过 ref 读取，避免调用方传内联函数导致 memo 失效
  const extraFilterRef = useRef(opts?.extraFilter)
  extraFilterRef.current = opts?.extraFilter

  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)

  // 过滤条件按内容参与 queryKey（对象字面量每渲染都是新引用，不能直接当依赖）
  const filters = opts?.filters
  const filterKey = JSON.stringify(filters ?? {})

  const listQuery = useQuery({
    queryKey: serverPaged
      ? [...queryKey, 'paged', page, pageSize, filterKey]
      : [...queryKey, 'list'],
    queryFn: async (): Promise<{ items: T[]; total: number }> => {
      if (serverPaged && api.listPaged) {
        return api.listPaged(Math.max(1, page), pageSize, filters)
      }
      const rows = await api.list()
      return { items: rows, total: rows.length }
    },
  })
  const items = useMemo(() => listQuery.data?.items ?? [], [listQuery.data])
  const serverTotal = listQuery.data?.total ?? 0

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

  const totalCount = serverPaged ? serverTotal : filtered.length
  const pageCount = Math.max(1, Math.ceil(totalCount / pageSize))
  const safePage = Math.min(page, pageCount)
  // 数据收缩后自动回到合法页（如删除末页最后一条后 total 变小）
  useEffect(() => {
    if (safePage !== page) setPage(safePage)
  }, [safePage, page])
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
    total: totalCount,
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
