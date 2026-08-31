# -*- coding: utf-8 -*-
"""
Domain Model V1 —— API 层验收测试（对应设计文档第 33-34 节验收标准）

前置：cms-server 已在本机 8088 端口运行且包含 0003_domain_model_v1 迁移。
流程：owner 登录 → 只通过 /api/v1/* 创建 Site/Theme/Templates/Contexts/
      Contents/Channels → 挂载 Context → 一稿多投 Channel →
      GET /api/v1/sites/:id/render?route=/ 校验 Presentation Contract。

可重复运行：所有创建动作前先按 slug 清理旧记录（通过 API DELETE）。
"""
import json
import sys
import urllib.request
import urllib.error

BASE = "http://127.0.0.1:8088"
USER = "owner"
PASSWORD = "demo1234"

TOKEN = ""
PASS = 0
FAIL = 0


def call(method, path, body=None, auth=True, expect=None):
    url = BASE + path
    data = json.dumps(body).encode("utf-8") if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Content-Type", "application/json")
    if auth and TOKEN:
        req.add_header("Authorization", "Bearer " + TOKEN)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            payload = json.loads(resp.read().decode("utf-8"))
            status = resp.status
    except urllib.error.HTTPError as e:
        payload = json.loads(e.read().decode("utf-8"))
        status = e.code
    if expect is not None and status != expect:
        raise AssertionError(f"{method} {path} -> {status} (期望 {expect}): {json.dumps(payload, ensure_ascii=False)[:300]}")
    return status, payload


def check(name, cond, detail=""):
    global PASS, FAIL
    if cond:
        PASS += 1
        print(f"[ok] {name}")
    else:
        FAIL += 1
        print(f"[FAIL] {name} {detail}")


def data_of(payload):
    return payload.get("data") if isinstance(payload, dict) else None


def find_by_slug(path, slug):
    _, p = call("GET", f"{path}?limit=200")
    items = data_of(p) or []
    if isinstance(items, dict):
        items = items.get("items") or items.get("list") or []
    for it in items:
        if it.get("slug") == slug:
            return it
    return None


def cleanup():
    """按 slug 删掉上一轮 seed 的记录，保证幂等。"""
    for path, slug in [
        ("/api/v1/content", "hello-coucouya"),
        ("/api/v1/content", "kosx-ai"),
        ("/api/v1/content", "coucouya-profile"),
        ("/api/v1/contexts", "cross-border"),
        ("/api/v1/contexts", "creator"),
        ("/api/v1/contexts", "web3"),
        ("/api/v1/contexts", "ai"),
        ("/api/v1/channels", "website"),
        ("/api/v1/channels", "x"),
        ("/api/v1/channels", "wechat"),
        ("/api/v1/channels", "telegram"),
        ("/api/v1/templates", "personal-brand-home"),
        ("/api/v1/templates", "article"),
        ("/api/v1/templates", "project"),
        ("/api/v1/templates", "product"),
        ("/api/v1/themes", "coucouya"),
        ("/api/v1/sites", "coucouya"),
    ]:
        found = find_by_slug(path, slug)
        if found:
            call("DELETE", f"{path}/{found['id']}", expect=200)


def main():
    global TOKEN

    # ── 0. 登录 ──
    _, p = call("POST", "/api/auth/login", {"username": USER, "password": PASSWORD}, auth=False, expect=200)
    d = data_of(p) or {}
    TOKEN = d.get("token") or d.get("accessToken") or ""
    check("owner 登录拿到 token", bool(TOKEN))

    cleanup()

    # ── 1. Site ──
    _, p = call("POST", "/api/v1/sites", {
        "name": "CouCouYa", "slug": "coucouya", "domain": "coucouya.com",
        "description": "个人品牌站", "defaultLocale": "zh-CN", "timezone": "Asia/Shanghai",
        "status": "published",
    }, expect=200)
    site_id = data_of(p)["id"]
    check("创建 Site: CouCouYa", bool(site_id))

    # ── 2. Theme + Version ──
    _, p = call("POST", "/api/v1/themes", {"name": "CouCouYa", "slug": "coucouya", "status": "published"}, expect=200)
    theme_id = data_of(p)["id"]
    _, p = call("POST", f"/api/v1/themes/{theme_id}/versions", {
        "tokens": {
            "color": {"background": "#fafaf9", "foreground": "#1c1917", "accent": "#d97706"},
            "typography": {"heading": "serif", "body": "sans-serif", "mono": "mono"},
            "spacing": {"section": "96px", "container": "1120px"},
            "radius": {"card": "12px", "button": "8px"},
        },
        "status": "published",
    }, expect=200)
    check("创建 Theme: CouCouYa + v1 tokens", True)
    _, p = call("POST", f"/api/v1/sites/{site_id}/themes", {"theme_id": theme_id, "is_default": True}, expect=200)
    check("绑定 Site 默认 Theme", True)

    # ── 3. Templates + Versions ──
    home_sections = [
        {"id": "hero", "component": "hero", "binding": {"source": "site.profile"}},
        {"id": "about", "component": "profile", "binding": {"source": "content", "type": "profile"}},
        {"id": "metrics", "component": "metrics", "binding": {"source": "profile.metrics"}},
        {"id": "organization", "component": "organization-list", "binding": {"source": "content", "type": "organization", "featured": True}},
        {"id": "writing", "component": "article-list", "binding": {"source": "content", "type": "article", "limit": 5}},
        {"id": "projects", "component": "project-list", "binding": {"source": "content", "type": "project"}},
        {"id": "products", "component": "product-list", "binding": {"source": "content", "type": "product"}},
    ]
    templates = {
        "personal-brand-home": ("PersonalBrandHome", "home", home_sections),
        "article": ("Article", "article", [{"id": "main", "component": "article", "binding": {"source": "content", "type": "article"}}]),
        "project": ("Project", "project", [{"id": "main", "component": "project", "binding": {"source": "content", "type": "project"}}]),
        "product": ("Product", "product", [{"id": "main", "component": "product", "binding": {"source": "content", "type": "product"}}]),
    }
    tpl_ids = {}
    for slug, (name, ttype, sections) in templates.items():
        _, p = call("POST", "/api/v1/templates", {"name": name, "slug": slug, "type": ttype, "status": "published"}, expect=200)
        tid = data_of(p)["id"]
        tpl_ids[slug] = tid
        call("POST", f"/api/v1/templates/{tid}/versions", {
            "definition": {"schema": "admin.template.v1", "type": "page", "sections": sections},
            "status": "published",
        }, expect=200)
    check("创建 4 个 Template + 已发布版本", len(tpl_ids) == 4)

    routes = [("personal-brand-home", "/", True), ("article", "/article/:slug", False),
              ("project", "/project/:slug", False), ("product", "/product/:slug", False)]
    for slug, route, default in routes:
        call("POST", f"/api/v1/sites/{site_id}/templates",
             {"template_id": tpl_ids[slug], "route": route, "is_default": default}, expect=200)
    check("绑定 Site 4 条路由", True)

    # ── 4. Contexts ──
    ctx_ids = {}
    for name, slug, extra in [
        ("AI", "ai", {"audience": "builder", "intent": "education"}),
        ("Web3", "web3", {"audience": "global", "industry": "crypto"}),
        ("Creator", "creator", {"industry": "creator-economy"}),
        ("Cross-border", "cross-border", {"region": "global"}),
    ]:
        body = {"name": name, "slug": slug, "type": "topic"}
        data = {"audience": extra.get("audience"), "intent": extra.get("intent"),
                "industry": extra.get("industry"), "region": extra.get("region")}
        body["data"] = {k: v for k, v in data.items() if v}
        _, p = call("POST", "/api/v1/contexts", body, expect=200)
        ctx_ids[slug] = data_of(p)["id"]
    check("创建 4 个 Context（含语义字段）", len(ctx_ids) == 4)

    # ── 5. Channels ──
    ch_ids = {}
    for name, ctype in [("Website", "website"), ("X", "x"), ("WeChat", "wechat"), ("Telegram", "newsletter")]:
        _, p = call("POST", "/api/v1/channels", {"name": name, "type": ctype, "url": f"https://{ctype}.example.com"}, expect=200)
        ch_ids[ctype] = data_of(p)["id"]
    check("创建 4 个 Channel", len(ch_ids) == 4)

    # ── 6. Contents ──
    contents = [
        {"type": "profile", "slug": "coucouya-profile", "title": "CouCouYa", "status": "published",
         "data": {"body": "Builder / Creator", "metrics": {"views": 1000, "likes": 50}}},
        {"type": "organization", "slug": "kosx-ai", "title": "KOSX.ai", "status": "published",
         "data": {"role": "current", "website": "https://kosx.ai"}},
        {"type": "article", "slug": "hello-coucouya", "title": "Hello CouCouYa", "status": "published",
         "summary": "首篇文章", "data": {"body": "Agent-first 内容平台的第一篇文章。"}},
    ]
    cids = {}
    for c in contents:
        _, p = call("POST", "/api/v1/content", c, expect=200)
        cids[c["slug"]] = data_of(p)["id"]
    check("创建 3 条 Content（profile/organization/article）", len(cids) == 3)

    # ── 7. Context 挂载（含 role/weight）──
    call("POST", f"/api/v1/content/{cids['hello-coucouya']}/contexts",
         {"context_id": ctx_ids["ai"], "role": "primary", "weight": 2.0}, expect=200)
    call("POST", f"/api/v1/content/{cids['hello-coucouya']}/contexts",
         {"context_id": ctx_ids["creator"], "role": "secondary", "weight": 1.0}, expect=200)
    call("POST", f"/api/v1/content/{cids['kosx-ai']}/contexts",
         {"context_id": ctx_ids["web3"], "role": "primary"}, expect=200)
    check("Content 挂载 Context（primary/secondary + weight）", True)

    # ── 8. 一稿多投 ──
    for ctype in ["website", "x", "newsletter"]:
        call("POST", f"/api/v1/content/{cids['hello-coucouya']}/channels/{ch_ids[ctype]}/publish",
             {"status": "published"}, expect=200)
    check("一稿多投：article → Website + X + Telegram", True)

    # ── 9. 交叉查询 ──
    _, p = call("GET", "/api/v1/content?context=ai")
    items = data_of(p) or []
    check("按 Context(ai) 交叉查询命中文章", any(i.get("id") == cids["hello-coucouya"] for i in items))

    # ── 10. Render Contract（验收核心）──
    _, p = call("GET", f"/api/v1/sites/{site_id}/render?route=/")
    r = data_of(p) or {}
    check("render 返回 schema=admin.render.v1", r.get("schema") == "admin.render.v1", f"实际 {r.get('schema')}")
    check("render.site.name=CouCouYa", (r.get("site") or {}).get("name") == "CouCouYa")
    sections = r.get("sections") or []
    check("render.sections 共 7 个组件", len(sections) == 7, f"实际 {len(sections)}")
    comps = [s.get("component") for s in sections]
    check("hero / article-list 等组件齐全", "hero" in comps and "article-list" in comps)
    theme = r.get("theme") or {}
    check("render.theme 携带 tokens", bool(theme.get("tokens")))
    check("render.template 携带 version", bool((r.get("template") or {}).get("version")))

    print(f"\n结果：{PASS} 通过 / {FAIL} 失败")
    sys.exit(1 if FAIL else 0)


if __name__ == "__main__":
    main()
