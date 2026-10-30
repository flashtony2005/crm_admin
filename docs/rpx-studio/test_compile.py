"""对 compiler.py 生成的每种骨架做真跑验证：lint / validate / run / 独立重算。

用法：python test_compile.py [csv路径]
"""
import io
import json
import os
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import compiler as C

RPX = r"F:\project\report\rpx\target\debug\rpx.exe"
ROOT = r"F:\tmp\studio-test"
CSV = sys.argv[1] if len(sys.argv) > 1 else r"F:\project\admin\docs\rpx-store-demo\data\store-sales.csv"

fail = 0


def ok(name, cond, info=""):
    global fail
    if not cond:
        fail += 1
    print(("  PASS  " if cond else "  FAIL  ") + name + ("" if cond else "  <<< " + str(info)))


raw = io.open(CSV, "rb").read()
parsed = C.parse(raw)
cols = C.analyze(parsed)
roles = C.guess_roles(cols)
print("列：", [(c["name"], c["type"], c["distinct"]) for c in cols])
print("角色：", roles)
print()

CASES = [
    ("rank", ["rank", "share", "flag", "total"]),
    ("rank", ["rank"]),
    ("rank", ["share", "flag"]),
    ("summary", None),
    ("list", None),
]

rows = parsed["rows"]
header = parsed["header"]


def truth(group_col, detail_col, meas_col):
    gi, di, mi = header.index(group_col), header.index(detail_col), header.index(meas_col)
    recs = [(r[gi], r[di], float(r[mi])) for r in rows]
    out = []
    for g, d, m in recs:
        same = [x[2] for x in recs if x[0] == g]
        rank = 1 + sum(1 for v in same if v > m)
        share = m / sum(same) if sum(same) else 0
        glob = m / sum(x[2] for x in recs)
        out.append((g, d, m, rank, share, glob))
    return out


for recipe, feats in CASES:
    label = "%s%s" % (recipe, "+" + ",".join(feats) if feats else "")
    work = os.path.join(ROOT, recipe + "-" + ("-".join(feats) if feats else "def"))
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(os.path.join(work, "data"), exist_ok=True)
    shutil.copy(CSV, os.path.join(work, "data", "store-sales.csv"))

    tpl, plan, shown = C.build(
        recipe, cols, roles, feats,
        {"file_name": "store-sales.csv", "encoding": parsed["encoding"],
         "delimiter": parsed["delimiter"], "row_count": parsed["row_count"]},
    )
    tpath = os.path.join(work, "template.json")
    io.open(tpath, "w", encoding="utf-8").write(json.dumps(tpl, ensure_ascii=False, indent=2))

    print("=" * 70)
    print(label, "| 画布", tpl["grid"], "| 列:", [s["head"] for s in shown])

    out = subprocess.run([RPX, "lint", tpath, "--data-dir", os.path.join(work, "data")],
                         capture_output=True, text=True, encoding="utf-8", errors="replace")
    lj = json.loads(out.stdout or "{}")
    warns = [i for i in (lj.get("data", {}).get("issues") or []) if i.get("level") in ("warning", "error")]
    ok("lint exit0 且无告警", out.returncode == 0 and not warns, warns[:2])

    out = subprocess.run([RPX, "template", "validate", tpath], capture_output=True, text=True,
                         encoding="utf-8", errors="replace")
    vj = json.loads(out.stdout or "{}")
    ok("validate valid", out.returncode == 0 and vj.get("data", {}).get("valid") is True,
       out.stdout[:200])

    out = subprocess.run([RPX, "run", tpath, "--data-dir", os.path.join(work, "data"),
                          "--out-dir", os.path.join(work, "run")],
                         capture_output=True, text=True, encoding="utf-8", errors="replace")
    rj = json.loads(out.stdout or "{}")
    if out.returncode != 0 or not rj.get("ok"):
        ok("run", False, out.stdout[:300])
        continue
    ok("run exit0", True)
    lay = rj["data"].get("layout") or {}
    print("        layout:", lay)

    cells = json.load(io.open(os.path.join(work, "run", "artifacts", "result.json"), encoding="utf-8"))["cells"]
    grid = {}
    for c in cells:
        grid.setdefault(c["row"], {})[c["col"]] = c["value"]["value"]

    print("        ---- 结果 ----")
    for r in sorted(grid):
        print("        %2d  %s" % (r, " | ".join(str(grid[r].get(i, ""))[:14] for i in range(1, tpl["grid"]["cols"] + 1))))

    # 独立重算（按实际列位，不假设固定三列）
    T = truth(roles["group"], roles["detail"], roles["measure"])
    keys = [x["key"] for x in shown]
    pos = {k: i + 1 for i, k in enumerate(keys)}
    bad = 0
    for i, (g, d, m, rank, share, glob) in enumerate(T):
        rr = 4 + i
        got = grid.get(rr, {})
        e = []
        if "group" in pos:
            e.append(got.get(pos["group"]) == g)
        e.append(got.get(pos["detail"]) == d)
        e.append(abs(float(got.get(pos["measure"], 0)) - m) < 1e-9)
        if "rank" in pos:
            e.append(int(got.get(pos["rank"])) == rank)
        if "share" in pos:
            e.append(abs(float(got.get(pos["share"])) - share) < 1e-9)
        if "share_global" in pos:
            e.append(abs(float(got.get(pos["share_global"])) - glob) < 1e-9)
        if "flag" in pos:
            e.append(got.get(pos["flag"]) == ("★ 组内第一" if rank == 1 else ""))
        if not all(e):
            bad += 1
            print("        MISMATCH row", rr, (g, d, m, rank), "got", got, e)
    ok("明细 %d 行逐格一致" % len(T), bad == 0, bad)

    if "total" in pos:
        tr = 4 + len(T)
        got_total = float(grid.get(tr, {}).get(pos["measure"], 0))
        exp_total = sum(x[2] for x in T)
        ok("合计行 %s = %s" % (got_total, exp_total), abs(got_total - exp_total) < 1e-9)

    if "share" in pos:
        groups = {}
        for i, (g, d, m, rank, share, glob) in enumerate(T):
            groups.setdefault(g, 0.0)
            groups[g] += share
        ok("每组占比合计=100%", all(abs(v - 1.0) < 1e-9 for v in groups.values()),
           {k: round(v, 6) for k, v in groups.items()})

print()
print("=" * 70)
print("FAIL 项：", fail)
sys.exit(1 if fail else 0)
