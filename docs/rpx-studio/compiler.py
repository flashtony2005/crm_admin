"""RPX 报表编译器：CSV 列 + 需求 → 可执行模板。

设计原则：零依赖、纯函数、可解释。每一步决策都写进 plan，供界面展示。
不猜语法：所有表达式都用 rpx 0.16.0 实跑验证过（见 test_compile.py）。
"""
from __future__ import annotations

import csv
import io
import re

DELIMS = [",", "\t", ";", "|"]
INT_RE = re.compile(r"^-?\d+$")
NUM_RE = re.compile(r"^-?(?:\d+(?:\.\d*)?|\.\d+)$")
DATE_RE = re.compile(r"^\d{4}[-/]\d{1,2}([-/]\d{1,2})?$")

GROUP_HINTS = ["大区", "区域", "地区", "部门", "科室", "类别", "分类", "团队", "班次",
               "region", "area", "dept", "department", "group", "category", "team", "class"]
DETAIL_HINTS = ["门店", "名称", "姓名", "产品", "客户", "项目", "科目", "店", "item", "store",
                "name", "product", "customer", "project", "sku", "title"]
MEASURE_HINTS = ["销售额", "金额", "工资", "合计", "数量", "费用", "收入", "成本", "余额",
                 "amount", "sales", "total", "qty", "value", "price", "cost", "salary", "num"]


# ---------------------------------------------------------------- 读取与推断

def sniff(raw: bytes):
    """编码 + 分隔符。返回 (text, encoding, delimiter)。"""
    text, encoding = None, None
    for enc, label in (("utf-8-sig", "UTF-8"), ("utf-8", "UTF-8"), ("gb18030", "GBK")):
        try:
            text = raw.decode(enc)
            encoding = label
            break
        except UnicodeDecodeError:
            continue
    if text is None:
        raise ValueError("无法识别文件编码，请另存为 UTF-8 后重试")

    head = next((ln for ln in text.splitlines() if ln.strip()), "")
    delimiter = max(DELIMS, key=lambda d: head.count(d))
    if head.count(delimiter) == 0:
        delimiter = ","
    return text, encoding, delimiter


def parse(raw: bytes):
    text, encoding, delimiter = sniff(raw)
    rows = [[(c or "").strip() for c in r] for r in csv.reader(io.StringIO(text), delimiter=delimiter)]
    rows = [r for r in rows if any(c for c in r)]
    if len(rows) < 2:
        raise ValueError("至少需要表头 + 1 行数据")
    header, body = rows[0], rows[1:]
    width = len(header)
    body = [(r + [""] * width)[:width] for r in body]
    return {"encoding": encoding, "delimiter": delimiter,
            "header": header, "rows": body, "row_count": len(body)}


def _col_type(values):
    vals = [v for v in values if v != ""]
    if not vals:
        return "empty"
    if all(INT_RE.match(v) for v in vals):
        return "int"
    if all(NUM_RE.match(v) for v in vals):
        return "decimal"
    if all(DATE_RE.match(v) for v in vals):
        return "date"
    return "text"


def analyze(parsed):
    header, rows = parsed["header"], parsed["rows"]
    cols = []
    for i, name in enumerate(header):
        values = [r[i] for r in rows]
        seen = {}
        for v in values:
            seen[v] = seen.get(v, 0) + 1
        cols.append({
            "index": i,
            "name": name or "列%d" % (i + 1),
            "type": _col_type(values),
            "distinct": len(seen),
            "filled": sum(1 for v in values if v != ""),
            "blank": sum(1 for v in values if v == ""),
            "sample": values[:3],
        })
    return cols


def _hits(name, hints):
    low = name.lower()
    return any(h in low for h in hints)


def guess_roles(cols):
    """给界面一个默认角色分配，用户可改。"""
    numeric = [c for c in cols if c["type"] in ("int", "decimal")]
    textish = [c for c in cols if c["type"] in ("text", "date", "empty")]

    measure = next((c for c in numeric if _hits(c["name"], MEASURE_HINTS)), None)
    if measure is None and numeric:
        measure = max(numeric, key=lambda c: c["distinct"])

    rest = [c for c in textish if c is not measure]
    group = next((c for c in rest if _hits(c["name"], GROUP_HINTS) and c["distinct"] > 1), None)
    if group is None and rest:
        cands = [c for c in rest if 1 < c["distinct"] < len(cols) * 0 + 10 ** 6]
        if len(rest) > 1 and cands:
            group = min(cands, key=lambda c: c["distinct"])

    detail = next((c for c in rest if c is not group and _hits(c["name"], DETAIL_HINTS)), None)
    if detail is None:
        others = [c for c in rest if c is not group]
        if others:
            detail = max(others, key=lambda c: c["distinct"])

    return {
        "group": group["name"] if group else None,
        "detail": detail["name"] if detail else (cols[0]["name"] if cols else None),
        "measure": measure["name"] if measure else None,
    }


# ---------------------------------------------------------------- 骨架

RECIPES = {
    "rank": {
        "id": "rank",
        "title": "分组排名表",
        "tagline": "组内排名 + 组内占比 + 组内第一",
        "desc": "同一维度下若干明细横向比较：谁在组里第一、占组内多少。",
        "needs_group": True,
        "features": ["rank", "share", "flag", "total"],
    },
    "summary": {
        "id": "summary",
        "title": "分组汇总表",
        "tagline": "按组罗列 + 全表占比 + 合计",
        "desc": "只想知道每组有哪些条目、各占整体多少，不排名次。",
        "needs_group": True,
        "features": ["share_global", "total"],
    },
    "list": {
        "id": "list",
        "title": "明细清单",
        "tagline": "原样罗列 + 合计行",
        "desc": "先要一张能打印、能对数的清单，不做分组计算。",
        "needs_group": False,
        "features": ["total"],
    },
}

FEATURE_LABEL = {
    "rank": "组内排名",
    "share": "组内占比",
    "share_global": "全表占比",
    "flag": "组内第一标记",
    "total": "合计行",
}


def col_letter(n: int) -> str:
    s = ""
    while n > 0:
        n, r = divmod(n - 1, 26)
        s = chr(65 + r) + s
    return s


def _measure_format(by_name, measure_name):
    return "#,##0" if by_name[measure_name]["type"] == "int" else "#,##0.00"


def build(recipe_id, cols, roles, features=None, meta=None):
    """返回 (template, plan, columns_shown)。plan 是给界面看的决策日志。"""
    meta = meta or {}
    recipe = RECIPES.get(recipe_id)
    if recipe is None:
        raise ValueError("未知骨架：%s" % recipe_id)

    by_name = {c["name"]: c for c in cols}
    g_name = roles.get("group") if recipe["needs_group"] else None
    d_name = roles.get("detail")
    m_name = roles.get("measure")

    for val, label in ((g_name, "分组列"), (d_name, "明细列"), (m_name, "数值列")):
        if val and val not in by_name:
            raise ValueError("%s「%s」不在数据列里" % (label, val))
    if recipe["needs_group"] and not g_name:
        raise ValueError("「%s」必须指定一个分组列" % recipe["title"])
    if not d_name:
        raise ValueError("必须指定一个明细列")
    if not m_name:
        raise ValueError("必须指定一个数值列（排名、占比、合计都要靠它）")
    if by_name[m_name]["type"] not in ("int", "decimal"):
        raise ValueError("数值列「%s」里含非数字，无法求和" % m_name)

    plan = []
    plan.append(("读取列", "共 %d 列：%s" % (
        len(cols), "、".join("%s(%s)" % (c["name"], c["type"]) for c in cols))))
    if g_name:
        plan.append(("定分组", "「%s」%d 个不同取值 → 作分组维" % (g_name, by_name[g_name]["distinct"])))
    plan.append(("定明细", "「%s」%d 行、%d 个不同值 → 作明细维"
                 % (d_name, meta.get("row_count", 0), by_name[d_name]["distinct"])))
    plan.append(("定度量", "「%s」是%s → 求和、排名、占比都基于它"
                 % (m_name, "整数" if by_name[m_name]["type"] == "int" else "小数")))

    feats = list(features) if features is not None else list(recipe["features"])
    if "flag" in feats and "rank" not in feats:
        feats.insert(0, "rank")
        plan.append(("补依赖", "「组内第一」标记要读名次，已自动补上排名列"))
    if "share" in feats and not g_name:
        feats.remove("share")
        feats.append("share_global")
        plan.append(("降级", "没有分组列，组内占比退回全表占比"))

    num_fmt = _measure_format(by_name, m_name)
    ds = meta.get("ds") or "ds1"
    title = meta.get("title") or recipe["title"]
    note = meta.get("note") or "数据 %d 行 · %d 列 · rpx 编译生成" % (
        meta.get("row_count", 0), len(cols))

    # ---- 列清单
    columns = []
    if g_name:
        columns.append({"key": "group", "head": g_name, "width": 14, "align": "left"})
    columns.append({"key": "detail", "head": d_name, "width": 20, "align": "left"})
    columns.append({"key": "measure", "head": m_name, "width": 16, "align": "right"})
    if "rank" in feats:
        columns.append({"key": "rank", "head": "组内排名", "width": 12, "align": "center"})
    if "share" in feats:
        columns.append({"key": "share", "head": "组内占比", "width": 20, "align": "right"})
    if "share_global" in feats:
        columns.append({"key": "share_global", "head": "占比", "width": 20, "align": "right"})
    if "flag" in feats:
        columns.append({"key": "flag", "head": "组内第一", "width": 18, "align": "center"})

    for i, c in enumerate(columns, start=1):
        c["col"] = i
        c["letter"] = col_letter(i)
    idx = {c["key"]: c for c in columns}

    n_cols = len(columns)
    has_total = "total" in feats
    has_note = bool(note)
    r_head = 3 if has_note else 2
    r_detail = r_head + 1
    r_total = r_detail + 1
    r_foot = r_total + 1 if has_total else r_detail + 1
    rows = r_foot if has_note else (r_total if has_total else r_detail)

    cells = []
    # 标题
    cells.append({"row": 1, "col": 1, "text": title, "font_size": 16, "bold": True,
                  "h_align": "center", "merge": {"rows": 1, "cols": n_cols}})
    if has_note:
        cells.append({"row": 2, "col": 1, "text": note, "font_size": 9,
                      "fore_color": "#666666", "merge": {"rows": 1, "cols": n_cols}})

    # 表头
    for c in columns:
        cells.append({"row": r_head, "col": c["col"], "text": c["head"], "bold": True,
                      "back_color": "#2C4A5A", "fore_color": "#FFFFFF", "h_align": "center"})

    # 明细行
    RD = r_detail
    if g_name:
        cells.append({"row": RD, "col": 1, "extend_mode": "vertical",
                      "expr": "%s.group(%s)" % (ds, g_name)})
        plan.append(("写主格", "A%d = %s.group(%s)：按分组维扩展，同值合并成一格"
                     % (RD, ds, g_name)))
        parent = "A%d" % RD
        for key in ("detail", "measure"):
            c = idx[key]
            cell = {"row": RD, "col": c["col"], "extend_mode": "vertical" if key == "detail" else "none",
                    "left_head": parent, "expr": "%s.select(%s)" % (ds, c["head"])}
            if key == "measure":
                cell.update({"format": num_fmt, "h_align": "right"})
            cells.append(cell)
            parent = "%s%d" % (c["letter"], RD)
        plan.append(("挂父子格", "B%d 挂 A%d、C%d 挂 B%d：明细随主格走，不会拉成笛卡尔积"
                     % (RD, RD, RD, RD)))
    else:
        c = idx["detail"]
        cells.append({"row": RD, "col": 1, "extend_mode": "vertical",
                      "expr": "%s.select(%s)" % (ds, c["head"])})
        c2 = idx["measure"]
        cells.append({"row": RD, "col": c2["col"], "extend_mode": "none", "left_head": "A%d" % RD,
                      "expr": "%s.select(%s)" % (ds, c2["head"]), "format": num_fmt,
                      "h_align": "right"})
        plan.append(("挂父子格", "B%d 挂 A%d：一列一行，按位置对齐" % (RD, RD)))
        parent = "A%d" % RD

    M = "%s%d" % (idx["measure"]["letter"], RD)
    if g_name:
        G = "%s%d" % (idx["group"]["letter"], RD)
        cond = "%s==$%s" % (G, G)
    else:
        cond = None

    # 计算列：统一挂明细列
    calc_parent = "%s%d" % (idx["detail"]["letter"], RD)
    for key in ("rank", "share", "share_global", "flag"):
        if key not in idx:
            continue
        c = idx[key]
        if key == "rank":
            expr = "count(%s[`0]{%s>$%s and %s})+1" % (M, M, M, cond)
            cells.append({"row": RD, "col": c["col"], "extend_mode": "none",
                          "left_head": calc_parent, "expr": expr, "h_align": "center"})
            plan.append(("算排名", "%s%d = count(%s[`0]{比本行大 且 同组})+1 —— 竞赛排名，并列后跳号"
                         % (c["letter"], RD, M)))
        elif key == "share":
            expr = "%s/sum(%s[`0]{%s})" % (M, M, cond)
            cells.append({"row": RD, "col": c["col"], "extend_mode": "none",
                          "left_head": calc_parent, "expr": expr, "format": "0.00%",
                          "h_align": "right"})
            plan.append(("算占比", "%s%d = 本行 / sum(同组)；`` `0 `` 先取全表集合再用条件收窄"
                         % (c["letter"], RD)))
        elif key == "share_global":
            expr = "%s/sum(%s[`0]{})" % (M, M)
            cells.append({"row": RD, "col": c["col"], "extend_mode": "none",
                          "left_head": calc_parent, "expr": expr, "format": "0.00%",
                          "h_align": "right"})
            plan.append(("算占比", "%s%d = 本行 / sum(全表)" % (c["letter"], RD)))
        elif key == "flag":
            rank = idx["rank"]
            expr = "if(%s%d==1,'★ 组内第一','')" % (rank["letter"], RD)
            cells.append({"row": RD, "col": c["col"], "extend_mode": "none",
                          "left_head": calc_parent, "expr": expr, "h_align": "center"})
            plan.append(("加标记", "%s%d = if(排名==1, '★ 组内第一', '')" % (c["letter"], RD)))

    # 合计行
    if has_total:
        mc = idx["measure"]
        total_head = {"row": r_total, "col": 1, "text": "合计", "bold": True, "h_align": "center"}
        # 1×1 合并会被判 TEMPLATE_INVALID（Excel 也不允许），只在跨列时才写 merge
        if mc["col"] - 1 >= 2:
            total_head["merge"] = {"rows": 1, "cols": mc["col"] - 1}
        cells.append(total_head)
        cells.append({"row": r_total, "col": mc["col"], "expr": "sum(%s[`0]{})" % M,
                      "format": num_fmt, "bold": True, "h_align": "right"})
        plan.append(("加合计", "%s%d = sum(%s[`0]{})：整块求和，不受分组条件影响"
                     % (mc["letter"], r_total, M)))

    if has_note:
        parts = []
        if "rank" in idx:
            parts.append("排名按同组比较、并列取竞赛排名")
        if "share" in idx:
            parts.append("占比分母为同组合计")
        elif "share_global" in idx:
            parts.append("占比分母为全表合计")
        if has_total:
            parts.append("合计行为全表汇总")
        foot = "说明：" + "；".join(parts) + "。"
        cells.append({"row": r_foot, "col": 1, "text": foot, "font_size": 9,
                      "fore_color": "#666666", "merge": {"rows": 1, "cols": n_cols}})

    template = {
        "version": 1,
        "grid": {"rows": rows, "cols": n_cols},
        "datasets": [{
            "name": ds,
            "file": {
                "file_name": meta.get("file_name", "data.csv"),
                "encoding": meta.get("encoding", "UTF-8"),
                "delimiter": meta.get("delimiter", ","),
            },
        }],
        "col_props": [{"col": c["col"], "width": c["width"]} for c in columns],
        "row_props": [p for p in (
            {"row": 1, "height": 30},
            {"row": r_head, "height": 22},
            {"row": r_total, "height": 22} if has_total else None,
        ) if p and p["row"] <= rows],
        "cells": cells,
    }
    plan.append(("定版面", "画布 %d 行 × %d 列；百分比列宽 ≥20mm 以避开 L008 告警"
                 % (rows, n_cols)))
    return template, plan, [{"head": c["head"], "key": c["key"], "letter": c["letter"]} for c in columns]
