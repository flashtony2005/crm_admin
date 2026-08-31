#!/usr/bin/env python3
"""Domain Model V1 —— 模型契约验证（不依赖 Rust 编译）。

用途
----
直接从 server/src/db.rs 抽取 `0003_domain_model_v1` 的 DDL，在内存库里建表，
然后把 CouCouYa 按 Domain Model V1 的结构 seed 进去，验证三件事：

1. 结构可执行 —— 所有 CREATE 语句通过，且**可重复执行**（幂等）。
2. 模型可用   —— CouCouYa（Site / Theme / 4 个 Template / Context /
                 Content / Channel + 绑定）能自然落进去，不需要绕。
   文档原话：「如果 CouCouYa 无法优雅地 seed 进去，说明我们的模型还有问题」。
3. 约束生效   —— 外键级联、复合主键、唯一索引按预期工作。

这只是**模型层**验证，不等于 Rust entity 正确；entity 由 cargo check 把关。
运行：python scripts/validate_domain_model_v1.py
"""

from __future__ import annotations

import json
import re
import sqlite3
import sys
from pathlib import Path

SERVER_SRC = Path(__file__).resolve().parent.parent / "server" / "src" / "db.rs"
MIGRATION_VERSION = "0003_domain_model_v1"

EXPECTED_TABLES = {
    "sites",
    "channels",
    "contents",
    "contexts",
    "content_contexts",
    "content_channels",
    "templates",
    "template_versions",
    "site_templates",
    "themes",
    "theme_versions",
    "site_themes",
}


def extract_ddl() -> list[str]:
    """从 db.rs 抽出指定迁移的 SQL 字面量（支持多行字符串）。"""
    src = SERVER_SRC.read_text(encoding="utf-8")
    anchor = f'version: "{MIGRATION_VERSION}"'
    if anchor not in src:
        sys.exit(f"[fail] 在 db.rs 中找不到迁移 {MIGRATION_VERSION}")
    start = src.index(anchor)
    block = src[start : src.index("];", start)]
    lits = re.findall(r'"((?:[^"\\]|\\.)*)"', block, re.S)
    return [
        s.replace('\\"', '"').strip()
        for s in lits
        if s.strip().upper().startswith(("CREATE", "ALTER", "DROP", "INSERT"))
    ]


def build_schema() -> sqlite3.Connection:
    sqls = extract_ddl()
    con = sqlite3.connect(":memory:")
    con.execute("PRAGMA foreign_keys=ON")
    for s in sqls:
        con.execute(s)
    # 幂等：整体再跑一遍，全部 IF NOT EXISTS
    for s in sqls:
        con.execute(s)
    return con


def seed_coucouya(con: sqlite3.Connection) -> None:
    """把 CouCouYa 落进 Domain Model V1。

    这一段的价值不在 demo，而在当模型契约测试：任何一处写起来别扭，
    都说明模型有问题。
    """
    now = "2026-08-31T21:00:00Z"

    # ── Site：只放身份与设置，不内联任何呈现 JSON ──
    con.execute(
        "INSERT INTO sites VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        (
            "site_coucouya",
            "CouCouYa",
            "coucouya",
            "coucouya.com",
            "个人品牌站：AI / Web3 / 跨境 / 创作者经济",
            "zh-CN",
            "Asia/Shanghai",
            "active",
            json.dumps({"mainPort": 8088}),
            "{}",
            now,
            now,
        ),
    )

    # ── Context：语义上下文，不是 Tag ──
    contexts = [
        ("ctx_ai", "topic", "AI", "ai", {"industry": "ai", "intent": "education"}),
        ("ctx_web3", "topic", "Web3", "web3", {"industry": "web3"}),
        ("ctx_creator", "topic", "Creator", "creator", {"audience": "creator"}),
        (
            "ctx_crossborder",
            "topic",
            "Cross-border",
            "cross-border",
            {"industry": "cross-border", "region": "global"},
        ),
    ]
    for cid, ctype, name, slug, data in contexts:
        con.execute(
            "INSERT INTO contexts VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            (cid, ctype, name, slug, None, json.dumps(data), "{}", None, "active", now, now),
        )

    # ── Theme + ThemeVersion：只决定「长什么样」──
    con.execute(
        "INSERT INTO themes VALUES (?,?,?,?,?,?,?,?)",
        ("theme_coucouya", "CouCouYa", "coucouya", "默认主题", "published", "{}", now, now),
    )
    tokens = {
        "schema": "admin.theme.v1",
        "tokens": {
            "color": {"background": "#ffffff", "foreground": "#111111", "accent": "#ff6a00"},
            "typography": {"heading": "Inter", "body": "Inter", "mono": "JetBrains Mono"},
            "spacing": {"section": "4rem", "container": "1200px"},
            "layout": {"maxWidth": "1200px", "grid": "12"},
            "radius": {"card": "12px", "button": "8px"},
        },
    }
    con.execute(
        "INSERT INTO theme_versions VALUES (?,?,?,?,?,?,?)",
        ("tv_1", "theme_coucouya", 1, json.dumps(tokens), "{}", "published", now),
    )
    con.execute(
        "INSERT INTO site_themes VALUES (?,?,?,?,?,?)",
        ("site_coucouya", "theme_coucouya", 1, "{}", now, now),
    )

    # ── Templates + Versions：只决定「怎么组合」──
    templates = [
        ("tpl_home", "PersonalBrandHome", "personal-brand-home", "home"),
        ("tpl_article", "Article", "article", "article"),
        ("tpl_project", "Project", "project", "project"),
        ("tpl_product", "Product", "product", "product"),
    ]
    for tid, name, slug, ttype in templates:
        con.execute(
            "INSERT INTO templates VALUES (?,?,?,?,?,?,?,?,?)",
            (tid, name, slug, ttype, None, "published", "{}", now, now),
        )

    home_definition = {
        "schema": "admin.template.v1",
        "type": "page",
        "sections": [
            {"id": "hero", "component": "hero", "binding": {"source": "site.profile"}},
            {
                "id": "about",
                "component": "profile",
                "binding": {"source": "content", "type": "profile"},
            },
            {
                "id": "metrics",
                "component": "metrics",
                "binding": {"source": "profile.metrics"},
            },
            {
                "id": "organization",
                "component": "organization-list",
                "binding": {"source": "content", "type": "organization", "featured": True},
            },
            {
                "id": "writing",
                "component": "article-list",
                "binding": {"source": "content", "type": "article", "limit": 5},
            },
            {
                "id": "projects",
                "component": "project-list",
                "binding": {"source": "content", "type": "project"},
            },
            {
                "id": "products",
                "component": "product-list",
                "binding": {"source": "content", "type": "product"},
            },
        ],
    }
    con.execute(
        "INSERT INTO template_versions VALUES (?,?,?,?,?,?)",
        ("tplv_home_1", "tpl_home", 1, json.dumps(home_definition), "published", now),
    )
    for tid, _, _, _ in [t for t in templates if t[0] != "tpl_home"]:
        con.execute(
            "INSERT INTO template_versions VALUES (?,?,?,?,?,?)",
            (f"{tid}_v1", tid, 1, json.dumps({"schema": "admin.template.v1", "sections": []}), "published", now),
        )

    # ── Site ↔ Template：路由绑定 ──
    routes = [
        ("tpl_home", "/", 1),
        ("tpl_article", "/article/:slug", 1),
        ("tpl_project", "/project/:slug", 1),
        ("tpl_product", "/product/:slug", 1),
    ]
    for tid, route, default in routes:
        con.execute(
            "INSERT INTO site_templates VALUES (?,?,?,?,?,?,?)",
            ("site_coucouya", tid, route, default, "{}", now, now),
        )

    # ── Channel：分发出口，不是站点部件 ──
    channels = [
        ("ch_website", "Website", "website", None, None, "https://coucouya.com"),
        ("ch_x", "X", "x", "twitter", "coucouya", "https://x.com/coucouya"),
        ("ch_wechat", "WeChat", "wechat", None, None, None),
        ("ch_telegram", "Telegram", "telegram", "telegram-bot", "coucouya_bot", None),
    ]
    for cid, name, ctype, prov, ext, url in channels:
        con.execute(
            "INSERT INTO channels VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            (cid, name, ctype, prov, ext, url, "{}", "{}", "active", now, now),
        )

    # ── Content：业务事实 ──
    contents = [
        (
            "ct_profile",
            "profile",
            None,
            "CouCouYa",
            "AI / Web3 / 跨境 / 创作者经济",
            {"body": "…", "metrics": {"views": 1000, "likes": 50}},
        ),
        (
            "ct_org_kosx",
            "organization",
            "kosx-ai",
            "KOSX.ai",
            "AI 内容基础设施",
            {"website": "https://kosx.ai", "logo": "", "role": "current"},
        ),
        (
            "ct_art_1",
            "article",
            "agent-first-cms",
            "为什么 CMS 需要 Agent-first",
            "从内容管理到内容生产",
            {"body": "…", "cover": "", "source_url": ""},
        ),
        (
            "ct_proj_1",
            "project",
            "coucouya-site",
            "CouCouYa 个人站",
            "Agent 驱动的内容平台",
            {"repo": "", "stack": ["rust", "react"]},
        ),
        (
            "ct_prod_1",
            "product",
            "kosx-pro",
            "KOSX Pro",
            "AI 内容生产套件",
            {"category": "ai-tool", "official_url": "https://kosx.ai", "affiliate_url": ""},
        ),
    ]
    for cid, ctype, slug, title, summary, data in contents:
        con.execute(
            "INSERT INTO contents VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (
                cid, ctype, slug, title, summary, "published", "zh-CN",
                json.dumps(data), "{}", None, 1, now, now, now,
            ),
        )

    # ── Content ↔ Context（带 role / weight）──
    for cid, ctx_id, role, weight in [
        ("ct_art_1", "ctx_ai", "primary", 1.0),
        ("ct_art_1", "ctx_creator", "audience", 0.8),
        ("ct_org_kosx", "ctx_ai", "primary", 1.0),
        ("ct_org_kosx", "ctx_crossborder", "secondary", 0.6),
        ("ct_prod_1", "ctx_ai", "primary", 1.0),
        ("ct_proj_1", "ctx_web3", "primary", 0.9),
    ]:
        con.execute(
            "INSERT INTO content_contexts VALUES (?,?,?,?,?)",
            (cid, ctx_id, role, weight, now),
        )

    # ── Content ↔ Channel：同一内容进多个渠道 ──
    for cid, ch_id in [
        ("ct_art_1", "ch_website"),
        ("ct_art_1", "ch_x"),
        ("ct_art_1", "ch_telegram"),
        ("ct_prod_1", "ch_website"),
    ]:
        con.execute(
            "INSERT INTO content_channels VALUES (?,?,?,?,?,?,?,?,?)",
            (cid, ch_id, None, None, "active", now, "{}", now, now),
        )

    con.commit()


def main() -> int:
    if not SERVER_SRC.exists():
        print(f"[skip] 找不到 {SERVER_SRC}")
        return 1

    con = build_schema()
    tables = {r[0] for r in con.execute("select name from sqlite_master where type='table'")}
    missing = EXPECTED_TABLES - tables
    if missing:
        print(f"[fail] 缺失表: {sorted(missing)}")
        return 1
    print(f"[ok] 12 张核心表全部就位，DDL 幂等（重复执行无报错）")

    seed_coucouya(con)

    # ── 契约 1：Site 的呈现绑定是可查询的，而不是内联 JSON ──
    routes = con.execute(
        "select route, template_id from site_templates where site_id=? order by route",
        ("site_coucouya",),
    ).fetchall()
    assert len(routes) == 4, f"路由绑定应为 4 条，实际 {len(routes)}"
    print(f"[ok] Site→Template 路由绑定 {len(routes)} 条: {[r[0] for r in routes]}")

    # ── 契约 2：Context 交叉查询（Analyst 的核心能力，不是 tag 匹配）──
    cross = con.execute(
        """
        select c.title from contents c
          join content_contexts cc on cc.content_id = c.id
          join contexts x on x.id = cc.context_id
        where x.slug in ('ai')
        group by c.id
        """,
    ).fetchall()
    assert cross, "Context 交叉查询无结果"
    print(f"[ok] Context 交叉查询可用，命中 {len(cross)} 条内容: {[r[0] for r in cross]}")

    # ── 契约 3：Content 多渠道分发 ──
    dist = con.execute(
        """
        select ch.name from content_channels cc
          join channels ch on ch.id = cc.channel_id
        where cc.content_id = 'ct_art_1' order by ch.name
        """
    ).fetchall()
    assert len(dist) == 3, f"内容应分发到 3 个渠道，实际 {len(dist)}"
    print(f"[ok] Content→Channel 一稿多投: {[r[0] for r in dist]}")

    # ── 契约 4：外键级联 ──
    con.execute("DELETE FROM contents WHERE id='ct_art_1'")
    left_cc = con.execute("select count(*) from content_contexts where content_id='ct_art_1'").fetchone()[0]
    left_ch = con.execute("select count(*) from content_channels where content_id='ct_art_1'").fetchone()[0]
    assert (left_cc, left_ch) == (0, 0), f"级联失败: contexts={left_cc} channels={left_ch}"
    print("[ok] 外键级联生效：删除 Content 后联结记录自动清理")

    # ── 契约 5：复合主键防重复 ──
    try:
        con.execute(
            "INSERT INTO content_contexts VALUES (?,?,?,?,?)",
            ("ct_org_kosx", "ctx_ai", "primary", 1.0, "2026-08-31T21:00:00Z"),
        )
        print("[fail] 复合主键未阻止重复挂载")
        return 1
    except sqlite3.IntegrityError:
        print("[ok] 复合主键生效：同一 Content 对同一 Context 不可重复挂载")

    # ── 契约 6：Channel provider/external_id 唯一 ──
    try:
        con.execute(
            "INSERT INTO channels VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            ("ch_dup", "X dup", "x", "twitter", "coucouya", None, "{}", "{}", "active", "now", "now"),
        )
        print("[fail] provider/external_id 唯一约束未生效")
        return 1
    except sqlite3.IntegrityError:
        print("[ok] Channel 唯一约束生效：同一外部渠道不可重复登记")

    # ── 契约 7：Render Contract 可被组装（外部 Renderer 消费的形态）──
    site = con.execute("select id,name,default_locale from sites where slug='coucouya'").fetchone()
    tpl = con.execute(
        """
        select t.id, tv.version, tv.definition_json from site_templates st
          join templates t on t.id = st.template_id
          join template_versions tv on tv.template_id = t.id
        where st.site_id=? and st.route='/' and st.is_default=1
        """,
        (site[0],),
    ).fetchone()
    theme = con.execute(
        """
        select tv.tokens_json from site_themes sth
          join theme_versions tv on tv.theme_id = sth.theme_id
        where sth.site_id=? and sth.is_default=1
        """,
        (site[0],),
    ).fetchone()
    definition = json.loads(tpl[2])
    assert definition["schema"] == "admin.template.v1"
    assert len(definition["sections"]) == 7
    assert json.loads(theme[0])["schema"] == "admin.theme.v1"
    print(
        f"[ok] Render Contract 可组装：site={site[1]} template={tpl[0]}@v{tpl[1]} "
        f"sections={len(definition['sections'])} theme=admin.theme.v1"
    )

    print("\n全部通过：Domain Model V1 可以装下 CouCouYa。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
