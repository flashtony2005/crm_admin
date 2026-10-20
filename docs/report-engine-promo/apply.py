# -*- coding: utf-8 -*-
"""把报表引擎推广内容灌入 admin 的 cms.db（幂等，可回滚）。

做四件事：
1. 备份原始 site_settings.home / site_title / site_tagline / home_pins(writing) 到 _original/
2. 插入 4 篇 published+featured 文章（封面用 /uploads/rpx-*.png）
3. 用 4 篇文章替换 home_pins 的 writing 槽
4. 更新 sections 的 site/about/org/lab/web3 五个 body（前端按 slug 浅合并覆盖 home）

回滚：把 _original/*.json 与 site_settings 的 home_backup_pre_rpx_20261019 写回即可。
"""
import json
import os
import sqlite3
import uuid
from datetime import datetime, timezone

BASE = os.path.dirname(os.path.abspath(__file__))
SERVER = os.path.abspath(os.path.join(BASE, "../../server"))
DB = os.path.join(SERVER, "cms.db")
ORIG = os.path.join(BASE, "_original")
NOW = "2026-10-19T09:10:00.000Z"

ARTICLES = [
    {
        "slug": "rpx-why-rust",
        "file": "articles/01-why-rust.html",
        "title": "把报表引擎重写成 Rust：我们为什么这么做，以及换来了什么",
        "summary": "部署重、启动慢、行为难钉住——我们把一套成熟的 Java 报表引擎重写成 Rust，用「夹具对拍」保证换语言不换语义。",
        "tags": "报表引擎,Rust,架构",
        "cover": "/uploads/rpx-architecture.png",
        "meta_title": "把报表引擎重写成 Rust：为什么、怎么保证语义、换来了什么",
        "meta_description": "AI 报表引擎的 Rust 重写实践：行为对拍、分层架构、单二进制部署与确定性输出。",
    },
    {
        "slug": "rpx-json-report-catalog",
        "file": "articles/02-json-report-catalog.html",
        "title": "一份 JSON，跑通 20 类中国式复杂报表",
        "summary": "多源分片、不规则分组、自由格间运算、交叉表合并、混合数据源、统计图、条码与典型财务报表：八类能力域与 20+ 份可运行样例。",
        "tags": "报表引擎,JSON,样例",
        "cover": "/uploads/rpx-capabilities.png",
        "meta_title": "一份 JSON 跑通 20 类中国式复杂报表：八类能力域盘点",
        "meta_description": "AI 报表引擎能力全景：分片、不规则分组、格间运算、动态合并、混合数据源、统计图、条码与财务报表。",
    },
    {
        "slug": "rpx-extend-cell",
        "file": "articles/03-extend-cell.html",
        "title": "扩展格：中国式报表的核心算法，到底怎么工作",
        "summary": "主格与从格、纵向与横向、带区与合计：讲清楚扩展格，以及为什么 sum(B3) 会算错、ds1.sum(salary) 才是对的。",
        "tags": "报表引擎,扩展格,算法",
        "cover": "/uploads/rpx-hero.png",
        "meta_title": "扩展格怎么工作：主格、从格、纵向横向与合计的正确写法",
        "meta_description": "从员工工资表示例讲清扩展格：extend_mode、主格从格、带区合计与 sum 的常见坑。",
    },
    {
        "slug": "rpx-json-cli-agent",
        "file": "articles/04-json-cli-agent.html",
        "title": "报表定义即代码：JSON 定义 + CLI + Agent",
        "summary": "把报表定义变成一份 JSON，把执行变成命令：lint、数据探针、capabilities、layout、run，让 AI 产出可验证的报表。",
        "tags": "报表引擎,JSON,CLI,Agent",
        "cover": "/uploads/rpx-architecture.png",
        "meta_title": "报表定义即代码：JSON + CLI + Agent 的工程闭环",
        "meta_description": "AI 报表引擎的 JSON 定义结构、CLI 能力与 Agent 工作流：可版本化、可执行、可验证的报表。",
    },
]

HERO = {
    "title": "把复杂报表，\n交给 AI 报表引擎。",
    "bio": "AI 报表引擎由 Rust 重写，把多源关联分片、不规则分组、自由格间运算、交叉表合并、统计图与条码，全部收敛到一份 JSON 定义；同一份定义，直接交付 Excel、HTML 与 PDF。\n\n20+ 份可运行样例，覆盖财务、保险、烟草、制造、政务等典型场景；每一份都是可回归的夹具。",
    "ctaLabel": "查看报表样例",
    "ctaHref": "/post/rpx-json-report-catalog",
}
STATS = [
    {"value": "20+", "label": "报表样例", "note": "覆盖 8 类中国式复杂报表"},
    {"value": "1", "label": "份 JSON 定义", "note": "数据源、公式、扩展、样式、打印设置"},
    {"value": "3", "label": "种导出", "note": "Excel / HTML / PDF 同源输出"},
]
PILLARS = {
    "eyebrow": "Why It / 为什么是它",
    "title": "把中国式复杂报表，做成可执行、可验证的定义。",
    "items": [
        {"no": "01", "title": "多源关联分片：纵向、横向与整片复制"},
        {"no": "02", "title": "不规则分组：分段、重叠与「其中」行"},
        {"no": "03", "title": "自由格间运算：同比环比、累计、排名与块级引用"},
        {"no": "04", "title": "交叉表动态合并：合并几行由数据决定"},
        {"no": "05", "title": "混合数据源：Excel / CSV / 双数据库同表关联"},
        {"no": "06", "title": "一次定义，交付 Excel / HTML / PDF"},
    ],
}
AFFILIATION = {
    "eyebrow": "Engine / 引擎",
    "name": "AI 报表引擎",
    "logo": "/uploads/rpx-mark.png",
    "desc": "引擎按职责分层：模型与计算内核零第三方依赖；数据源、导出、运行时与 HTTP 服务单向依赖；同一份定义在 CLI、服务与 Agent 中走同一条执行链。",
    "ctaLabel": "查看架构海报",
    "ctaHref": "/uploads/rpx-architecture.png",
}
WRITING = {
    "eyebrow": "Selected Writing / 技术文章",
    "title": "报表引擎的工程实践",
    "desc": "从 Rust 重写、JSON 定义、扩展格算法到 CLI 与 Agent：把报表引擎的取舍、坑位与验证方法写清楚。",
    "ctaLabel": "查看全部文章",
    "ctaHref": "/post/rpx-why-rust",
}
SERIES = {
    "eyebrow": "Deep Dives / 深入阅读",
    "title": "四篇读懂 AI 报表引擎",
    "desc": "按「为什么重写 → 能做什么 → 核心算法 → 定义即代码」的顺序，完整了解这套引擎。",
    "items": [
        {"no": "01", "title": "把报表引擎重写成 Rust", "desc": "为什么重写、怎么保证换语言不换语义、换来了什么。", "href": "/post/rpx-why-rust"},
        {"no": "02", "title": "一份 JSON，跑通 20 类复杂报表", "desc": "八类能力域与 20+ 份可运行样例的逐类盘点。", "href": "/post/rpx-json-report-catalog"},
        {"no": "03", "title": "扩展格到底怎么工作", "desc": "主格与从格、纵向与横向，以及合计为什么容易写错。", "href": "/post/rpx-extend-cell"},
        {"no": "04", "title": "报表定义即代码", "desc": "JSON + CLI + Agent：让报表可版本化、可执行、可验证。", "href": "/post/rpx-json-cli-agent"},
    ],
}
WEB3 = {
    "title": "快速开始",
    "desc": "一份 JSON 定义 + 一条命令，就能把报表跑起来；需要嵌入时，引擎是库，不是黑盒。",
    "disclaimer": "示例数据与模板均可在仓库中直接打开、修改、另存为起点。",
    "items": [
        {"name": "JSON 定义", "desc": "数据集、公式、扩展方向、样式与打印设置，一份文件描述完整报表。", "ctaLabel": "阅读定义规范", "ctaHref": "/post/rpx-json-cli-agent"},
        {"name": "CLI 与 Agent", "desc": "lint、数据探针、capabilities、layout、run：报表可以像构建产物一样被自动化。", "ctaLabel": "查看 CLI 能力", "ctaHref": "/post/rpx-json-cli-agent"},
    ],
}
TAGLINE = "Define once, run anywhere."


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def dump(path, obj):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent=2)


def main():
    os.makedirs(ORIG, exist_ok=True)
    con = sqlite3.connect(DB)
    con.row_factory = sqlite3.Row
    cur = con.cursor()

    # ---- 备份（write-once：只在首次生成，重跑不覆盖原始值）----
    def backup_once(name, obj):
        p = os.path.join(ORIG, name)
        if not os.path.exists(p):
            dump(p, obj)

    home_row = cur.execute("select value from site_settings where key='home'").fetchone()
    backup_once("site_settings_home.json", json.loads(home_row["value"]) if home_row else {})
    orig_home = load(os.path.join(ORIG, "site_settings_home.json"))
    for key in ("site_title", "site_tagline"):
        row = cur.execute("select value from site_settings where key=?", (key,)).fetchone()
        backup_once(f"site_settings_{key}.json", {"value": row["value"] if row else None})
    pins = [dict(r) for r in cur.execute("select * from home_pins where slot='writing'")]
    backup_once("home_pins_writing.json", pins)

    # ---- 文章（先删同 slug，保证幂等）----
    article_ids = {}
    for a in ARTICLES:
        cur.execute("delete from articles where slug=?", (a["slug"],))
        aid = str(uuid.uuid4())
        article_ids[a["slug"]] = aid
        content = open(os.path.join(BASE, a["file"]), encoding="utf-8").read()
        cur.execute(
            """insert into articles
               (id, tenant_id, title, summary, content, status, author, tags, created_at, updated_at,
                featured_image, published_at, meta_title, meta_description, slug, locale, views,
                visibility, featured, scheduled_at, canonical_url, paid_level, price_points, kind)
               values (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)""",
            (aid, "t_demo", a["title"], a["summary"], content, "published", "AI 报表引擎团队", a["tags"],
             NOW, NOW, a["cover"], NOW, a["title"], a["meta_description"], a["slug"], "zh", 0,
             "public", 1, "", "", 0, 0, "post"),
        )

    # ---- home_pins：writing 槽换成 4 篇新文章 ----
    cur.execute("delete from home_pins where slot='writing'")
    for i, a in enumerate(ARTICLES, start=1):
        cur.execute(
            "insert into home_pins (id, tenant_id, slot, article_id, sort, enabled, created_at, updated_at) values (?,?,?,?,?,?,?,?)",
            (str(uuid.uuid4()), "t_demo", "writing", article_ids[a["slug"]], i, 1, NOW, NOW),
        )

    # ---- nav_links：页脚 3 条换成推广入口（前端优先读这张表，不换会盖住 content.footer）----
    nav_rows = [dict(r) for r in cur.execute("select * from nav_links where grp='footer' order by sort")]
    backup_once("nav_links_footer.json", nav_rows)
    FOOTER_LINKS = [
        ("技术文章", "/post/rpx-why-rust", ""),
        ("能力海报", "/uploads/rpx-capabilities.png", "_blank"),
        ("架构海报", "/uploads/rpx-architecture.png", "_blank"),
    ]
    for i, (label, href, target) in enumerate(FOOTER_LINKS):
        if i < len(nav_rows):
            cur.execute("update nav_links set label=?, href=?, target=?, updated_at=? where id=?",
                        (label, href, target, NOW, nav_rows[i]["id"]))
        else:
            cur.execute(
                "insert into nav_links (id, tenant_id, grp, label, href, target, sort, enabled, created_at, updated_at) values (?,?,?,?,?,?,?,?,?,?)",
                (str(uuid.uuid4()), "t_demo", "footer", label, href, target, i, 1, NOW, NOW),
            )

    # ---- sections 五个 body ----
    site = load(os.path.join(ORIG, "site.json"))
    site.update({
        "brand": "AI 报表引擎",
        "brandEn": "AI Report Engine",
        "nav": {
            "links": [
                {"label": "产品", "href": "#about"},
                {"label": "能力", "href": "#affiliation"},
                {"label": "文章", "href": "#writing"},
                {"label": "深入", "href": "#series"},
                {"label": "快速开始", "href": "#web3"},
            ],
            "ctaLabel": "查看样例",
        },
        "footer": {
            "note": "© 2026 AI Report Engine · 一份 JSON，跑通中国式复杂报表。",
            "links": [
                {"label": "技术文章", "href": "/post/rpx-why-rust"},
                {"label": "能力海报", "href": "/uploads/rpx-capabilities.png"},
                {"label": "架构海报", "href": "/uploads/rpx-architecture.png"},
            ],
        },
    })
    about = load(os.path.join(ORIG, "about.json"))
    about.update({"hero": HERO, "kicker": "AI 报表引擎 · Rust 内核", "stats": STATS, "tagline": TAGLINE})
    org = load(os.path.join(ORIG, "org.json"))
    org.update({"affiliation": AFFILIATION, "pillars": PILLARS})
    lab = load(os.path.join(ORIG, "lab.json"))
    lab.update({"writing": WRITING, "series": SERIES})
    web3 = load(os.path.join(ORIG, "web3.json"))
    web3.update({"web3": WEB3})

    for slug, body in (("site", site), ("about", about), ("org", org), ("lab", lab), ("web3", web3)):
        cur.execute("update sections set body=?, updated_at=? where slug=?",
                    (json.dumps(body, ensure_ascii=False), NOW, slug))

    # ---- site_settings ----
    home = dict(orig_home)
    home.update({
        "brand": "AI 报表引擎", "brandEn": "AI Report Engine", "tagline": TAGLINE,
        "kicker": "AI 报表引擎 · Rust 内核", "hero": HERO, "stats": STATS,
        "affiliation": AFFILIATION, "pillars": PILLARS, "writing": WRITING,
        "series": SERIES, "web3": WEB3, "footer": site["footer"],
    })
    def set_setting(key, value, allow_insert=True):
        """只改 value/updated_at，保留 tenant_id / created_at；不存在才补一行。"""
        cur.execute("update site_settings set value=?, updated_at=? where key=?", (value, NOW, key))
        if cur.rowcount == 0 and allow_insert:
            cur.execute(
                "insert into site_settings (key, value, tenant_id, created_at, updated_at) values (?,?,?,?,?)",
                (key, value, "t_demo", NOW, NOW),
            )

    set_setting("home", json.dumps(home, ensure_ascii=False))
    if cur.execute("select 1 from site_settings where key='home_backup_pre_rpx_20261019'").fetchone() is None:
        set_setting("home_backup_pre_rpx_20261019", json.dumps(orig_home, ensure_ascii=False))
    set_setting("site_title", "AI 报表引擎")
    set_setting("site_tagline", "一份 JSON，跑通中国式复杂报表。")

    con.commit()
    con.close()
    print("OK: 4 articles + 4 pins + 5 sections + site_settings updated")
    print("article ids:", json.dumps(article_ids, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
