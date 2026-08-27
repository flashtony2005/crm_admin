import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { Button } from '@heroui/react'
import { membersApi, type Member } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

function MembersPage() {
  const [search, setSearch] = useState('')
  const t = useCmsCollection(membersApi, ['cms-members'], { searchFields: ['email', 'name', 'plan'] })

  const columns: CmsColumn<Member>[] = [
    { id: 'email', header: '邮箱', render: (r) => <span className="font-medium">{r.email}</span> },
    { id: 'name', header: '昵称', render: (r) => <span>{r.name || '—'}</span> },
    {
      id: 'plan', header: '套餐', render: (r) =>
        <span className={`px-2 py-0.5 rounded-full text-xs ${r.plan === 'free' ? 'bg-gray-100 text-gray-600' : 'bg-purple-100 text-purple-700'}`}>{r.plan}</span>,
    },
    { id: 'status', header: '状态', render: (r) => <span>{r.status === 1 ? '正常' : '停用'}</span> },
    { id: 'createdAt', header: '注册时间', render: (r) => <time className="text-xs text-os-text-muted">{fmtDate(r.createdAt)}</time> },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="会员" desc="注册会员列表与套餐分布。会员可在公开站注册、登录并访问会员专属内容。" />
      <CmsToolbar searchPlaceholder="搜索邮箱 / 昵称 / 套餐…" searchValue={search} onSearchChange={setSearch}>
        <Auth perm={P.contentMembersCreate}>
          <Button variant="primary" size="sm" onPress={() => alert('会员通过公开站 /membership 自助注册')}>说明</Button>
        </Auth>
      </CmsToolbar>
      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="👥"
        emptyTitle="还没有会员"
        emptyHint="会员在公开站的「会员」页面自助注册"
      />
    </div>
  )
}

export const Route = createFileRoute('/content/members')({ component: MembersPage })
