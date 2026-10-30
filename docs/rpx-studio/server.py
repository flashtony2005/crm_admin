"""RPX 报表工作台 —— 本地服务（零依赖，只用标准库）。

启动：python server.py [--port 8777]
然后浏览器打开 http://127.0.0.1:8777

它做的事：接住一个 CSV → 推断列角色 → 用编译器生成 rpx 模板 → 真调 rpx 引擎
lint / validate / run → 把结果表格、诊断、产物回给页面。
全程本地：不联网、不调用任何外部模型函数。
"""
from __future__ import annotations

import argparse
import io
import json
import os
import re
import shutil
import subprocess
import sys
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, unquote, urlparse

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import compiler as C

HERE = os.path.dirname(os.path.abspath(__file__))
WORK = os.path.join(HERE, "work")
RPX = os.environ.get("RPX_BIN") or r"F:\project\report\rpx\target\debug\rpx.exe"
CREATE_NO_WINDOW = 0x08000000 if os.name == "nt" else 0
TOKEN_RE = re.compile(r"^[0-9a-f]{16}$")

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")


# ---------------------------------------------------------------- 需求解析

INTENTS = [
    ("rank", ["排名", "名次", "第几名", "排位", "排序"]),
    ("share_global", ["全表占比", "整体占比", "全体占比", "占整体", "占全部"]),
    ("share", ["组内占比", "占比", "比重", "百分比", "比例", "份额"]),
    ("flag", ["组内第一", "第一名", "冠军", "标出", "标记", "标注"]),
    ("total", ["合计", "总计", "汇总", "总额", "小计"]),
]


GLOBAL_WORDS = ["全表", "整体", "全体"]
GROUP_WORDS = ["组内", "同组", "本组"]


def detect_intent(text, has_group):
    """自然语言 → 功能集合。返回 (features, matched[list of (feature, word)])"""
    if not text:
        return [], []
    # 先看是不是在说全表口径：说了全表/整体，占比就只能是全表占比
    if any(w in text for w in GLOBAL_WORDS) and any(
            w in text for w in ["占比", "比例", "比重", "百分", "份额"]):
        return ["share_global"], [("share_global", next(w for w in GLOBAL_WORDS if w in text))]

    feats, matched = [], []
    for fid, words in INTENTS:
        hit = next((w for w in words if w in text), None)
        if not hit:
            continue
        if fid == "share" and not has_group:
            fid = "share_global"
        if fid not in feats:
            feats.append(fid)
            matched.append((fid, hit))
    # 「全表占比」命中时，去掉被顺带匹配的「占比」
    if "share_global" in feats and "share" in feats:
        shared = any(w in text for w in ["组内占比"])
        if not shared:
            feats.remove("share")
            matched = [m for m in matched if m[0] != "share"]
    return feats, matched


def detect_columns(text, cols):
    """需求里点名的列，用来纠正角色分配。"""
    out = {}
    if not text:
        return out
    for c in cols:
        name = c["name"]
        if len(name) < 2 or name not in text:
            continue
        if re.search(r"按\s*%s|%s\s*分组|%s\s*分组" % (re.escape(name), re.escape(name), re.escape(name)), text):
            if c["type"] not in ("int", "decimal"):
                out["group"] = name
        if re.search(r"(金额|数值|度量|按)\s*%s|%s\s*(求和|合计|汇总|排名|占比)" % (re.escape(name), re.escape(name)), text):
            if c["type"] in ("int", "decimal"):
                out["measure"] = name
    return out


# ---------------------------------------------------------------- rpx 调用

def call_rpx(args, cwd=None, timeout=120):
    t0 = time.time()
    proc = subprocess.run([RPX] + args, cwd=cwd, capture_output=True, timeout=timeout,
                          creationflags=CREATE_NO_WINDOW)
    out = proc.stdout.decode("utf-8", "replace").strip()
    err = proc.stderr.decode("utf-8", "replace").strip()
    payload = None
    if out:
        try:
            payload = json.loads(out)
        except json.JSONDecodeError:
            payload = None
    return {
        "exit": proc.returncode,
        "json": payload,
        "stdout": out[:4000],
        "stderr": err[:2000],
        "ms": int((time.time() - t0) * 1000),
    }


# ---------------------------------------------------------------- 会话

def session_dir(token):
    if not TOKEN_RE.match(token or ""):
        raise ValueError("会话标识不合法")
    path = os.path.join(WORK, token)
    if not os.path.isdir(path):
        raise ValueError("会话不存在或已过期，请重新上传数据")
    return path


def fmt_from_template(template):
    """列 → 数字格式，供前端渲染。"""
    out = {}
    for c in template.get("cells", []):
        if c.get("format"):
            out[str(c["col"])] = c["format"]
    return out


# ---------------------------------------------------------------- HTTP

class Handler(BaseHTTPRequestHandler):
    server_version = "rpx-studio/1.0"

    def log_message(self, fmt, *args):
        sys.stdout.write("  · %s\n" % (fmt % args))

    # -- helpers
    def _send(self, code, body, ctype="application/json; charset=utf-8", extra=None):
        data = body if isinstance(body, bytes) else body.encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        for k, v in (extra or {}).items():
            self.send_header(k, v)
        self.end_headers()
        try:
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def _json(self, obj, code=200):
        self._send(code, json.dumps(obj, ensure_ascii=False), "application/json; charset=utf-8")

    def _body(self):
        n = int(self.headers.get("Content-Length") or 0)
        return self.rfile.read(n) if n else b""

    # -- routes
    def do_GET(self):
        parsed = urlparse(self.path)
        path, qs = parsed.path, parse_qs(parsed.query)
        if path in ("/", "/index.html"):
            fp = os.path.join(HERE, "index.html")
            if not os.path.isfile(fp):
                return self._send(500, "index.html 缺失", "text/plain; charset=utf-8")
            return self._send(200, io.open(fp, "rb").read(), "text/html; charset=utf-8")
        if path == "/api/recipes":
            return self._json({"ok": True, "recipes": list(C.RECIPES.values()),
                               "features": C.FEATURE_LABEL,
                               "rpx": RPX, "rpx_exists": os.path.isfile(RPX)})
        if path == "/api/download":
            try:
                token = (qs.get("token") or [""])[0]
                name = (qs.get("f") or [""])[0]
                sdir = session_dir(token)
                safe = os.path.basename(name)
                fp = os.path.join(sdir, "run", "artifacts", safe)
                if not os.path.isfile(fp):
                    return self._json({"ok": False, "error": {"code": "FILE_NOT_FOUND",
                                                              "message": "产物不存在：%s" % safe}}, 404)
                ctype = {
                    ".csv": "text/csv; charset=utf-8",
                    ".json": "application/json; charset=utf-8",
                    ".xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                }.get(os.path.splitext(safe)[1], "application/octet-stream")
                return self._send(200, io.open(fp, "rb").read(), ctype,
                                  {"Content-Disposition": "attachment; filename*=UTF-8''%s" % safe})
            except ValueError as e:
                return self._json({"ok": False, "error": {"code": "BAD_SESSION", "message": str(e)}}, 400)
        if path == "/api/sample":
            for cand in (os.path.join(HERE, "..", "rpx-store-demo", "data", "store-sales.csv"),
                         os.path.join(HERE, "sample", "store-sales.csv")):
                fp = os.path.abspath(cand)
                if os.path.isfile(fp):
                    return self._send(200, io.open(fp, "rb").read(), "text/csv; charset=utf-8")
            return self._json({"ok": False, "error": {"code": "NO_SAMPLE",
                                                      "message": "示例数据不在本机"}}, 404)
        if path == "/api/health":
            return self._json({"ok": True, "rpx": RPX, "rpx_exists": os.path.isfile(RPX)})
        return self._send(404, "not found", "text/plain; charset=utf-8")

    def do_POST(self):
        path = urlparse(self.path).path
        try:
            if path == "/api/upload":
                return self.api_upload()
            if path == "/api/compile":
                return self.api_compile()
            if path == "/api/run":
                return self.api_run()
            return self._json({"ok": False, "error": {"code": "NOT_FOUND", "message": "未知接口"}}, 404)
        except ValueError as e:
            return self._json({"ok": False, "error": {"code": "BAD_REQUEST", "message": str(e)}}, 400)
        except subprocess.TimeoutExpired:
            return self._json({"ok": False, "error": {"code": "ENGINE_TIMEOUT",
                                                      "message": "rpx 执行超时（>120s）"}}, 504)
        except Exception as e:  # noqa: BLE001
            import traceback
            traceback.print_exc()
            return self._json({"ok": False, "error": {"code": "INTERNAL",
                                                      "message": "%s: %s" % (type(e).__name__, e)}}, 500)

    # -- api
    def api_upload(self):
        raw = self._body()
        if not raw:
            raise ValueError("没有收到文件内容")
        filename = unquote(self.headers.get("X-Filename") or "data.csv")
        filename = os.path.basename(filename) or "data.csv"
        if len(raw) > 8 * 1024 * 1024:
            raise ValueError("文件超过 8MB，请先裁剪样本")

        parsed = C.parse(raw)
        cols = C.analyze(parsed)
        roles = C.guess_roles(cols)

        token = uuid.uuid4().hex[:16]
        sdir = os.path.join(WORK, token)
        os.makedirs(os.path.join(sdir, "data"), exist_ok=True)
        io.open(os.path.join(sdir, "data", "data.csv"), "wb").write(raw)
        meta = {"filename": filename, "encoding": parsed["encoding"],
                "delimiter": parsed["delimiter"], "row_count": parsed["row_count"],
                "cols": cols, "roles": roles, "created": time.time()}
        io.open(os.path.join(sdir, "meta.json"), "w", encoding="utf-8").write(
            json.dumps(meta, ensure_ascii=False, indent=1))

        self._json({
            "ok": True,
            "token": token,
            "filename": filename,
            "encoding": parsed["encoding"],
            "delimiter": parsed["delimiter"],
            "row_count": parsed["row_count"],
            "header": parsed["header"],
            "cols": cols,
            "roles": roles,
            "preview": parsed["rows"][:8],
        })

    def api_compile(self):
        body = json.loads(self._body() or b"{}")
        token = body.get("token") or ""
        sdir = session_dir(token)
        meta = json.loads(io.open(os.path.join(sdir, "meta.json"), encoding="utf-8").read())

        recipe = body.get("recipe") or "rank"
        cols = meta["cols"]
        roles = dict(meta["roles"])
        roles.update({k: v for k, v in (body.get("roles") or {}).items() if v})
        need = (body.get("need") or "").strip()
        title = (body.get("title") or "").strip()

        touched = body.get("features")
        base = list(C.RECIPES[recipe]["features"]) if touched is None else list(touched)

        overrides = detect_columns(need, cols)
        role_note = []
        for k, v in overrides.items():
            if roles.get(k) != v:
                role_note.append({"role": k, "from": roles.get(k), "to": v})
                roles[k] = v

        feats, matched = detect_intent(need, bool(roles.get("group")))
        words = {m[0]: m[1] for m in matched}
        merged = []
        for f in base + feats:
            if f not in merged:
                merged.append(f)
        if "share" in merged and "share_global" in merged:
            if any(w in need for w in GROUP_WORDS):
                merged.remove("share_global")
            elif any(w in need for w in GLOBAL_WORDS):
                merged.remove("share")
            else:
                # 需求没表态，就听骨架的默认口径，去掉后加进来的那个
                merged.remove("share" if "share_global" in base else "share_global")
        if "flag" in merged and "rank" not in merged:
            merged.insert(0, "rank")

        tpl, plan, shown = C.build(
            recipe, cols, roles, merged,
            {"file_name": "data.csv", "encoding": meta["encoding"],
             "delimiter": meta["delimiter"], "row_count": meta["row_count"],
             "title": title or None},
        )
        # 记下用于解释的附加信息
        extra = []
        if role_note:
            for n in role_note:
                extra.append(("改角色", "需求里点名了「%s」→ 由%s改为%s" % (
                    n["to"], _role_label(n["from"]), _role_label(n["to"]))))
        if words:
            extra.append(("读需求", "识别到：%s" % "、".join(
                "%s（“%s”）" % (C.FEATURE_LABEL.get(k, k), v) for k, v in words.items())))

        self._json({
            "ok": True,
            "template": tpl,
            "plan": [{"step": s, "detail": d} for s, d in (plan[:1] + extra + plan[1:])],
            "columns": shown,
            "features": merged,
            "roles": roles,
            "formats": fmt_from_template(tpl),
            "grid": tpl["grid"],
        })

    def api_run(self):
        body = json.loads(self._body() or b"{}")
        token = body.get("token") or ""
        tpl = body.get("template") or {}
        sdir = session_dir(token)
        if not tpl.get("cells"):
            raise ValueError("模板为空，先编译")

        tpath = os.path.join(sdir, "template.json")
        io.open(tpath, "w", encoding="utf-8").write(json.dumps(tpl, ensure_ascii=False, indent=2))
        data_dir = os.path.join(sdir, "data")
        run_dir = os.path.join(sdir, "run")
        shutil.rmtree(run_dir, ignore_errors=True)

        lint = call_rpx(["lint", tpath, "--data-dir", data_dir])
        val = call_rpx(["template", "validate", tpath])
        run = call_rpx(["run", tpath, "--data-dir", data_dir, "--out-dir", run_dir,
                        "--export", "csv", "--export", "xlsx", "--export", "print-xlsx"])

        lint_issues = []
        if lint["json"]:
            for it in (lint["json"].get("data") or {}).get("issues") or []:
                lint_issues.append({"code": it.get("code"), "level": it.get("level"),
                                    "message": it.get("message"), "cell": it.get("cell")})
        val_data = (val["json"] or {}).get("data") or {}

        resp = {
            "ok": bool(run["exit"] == 0 and (run["json"] or {}).get("ok")),
            "lint": {"exit": lint["exit"], "issues": lint_issues,
                     "warnings": sum(1 for i in lint_issues if i["level"] in ("warning", "error")),
                     "ms": lint["ms"]},
            "validate": {"exit": val["exit"], "valid": val_data.get("valid"),
                         "checked": val_data.get("checked") or [],
                         "deferred": val_data.get("deferred") or [],
                         "ms": val["ms"]},
            "run": {"exit": run["exit"], "ms": run["ms"],
                    "layout": ((run["json"] or {}).get("data") or {}).get("layout"),
                    "diagnostics": (run["json"] or {}).get("diagnostics") or [],
                    "error": (run["json"] or {}).get("error")},
            "files": [],
        }

        if not resp["ok"]:
            err = resp["run"]["error"] or {}
            resp["error"] = {"code": err.get("code") or "RUN_FAILED",
                             "message": err.get("message") or run["stdout"][:300] or "运行失败",
                             "pointer": err.get("json_pointer"),
                             "hint": err.get("hint")}
            return self._json(resp)

        art = os.path.join(run_dir, "artifacts")
        result = json.loads(io.open(os.path.join(art, "result.json"), encoding="utf-8").read())
        # 注意：origin 是「这个格由模板哪个格扩展而来」，不是合并位置。
        # 扩展出来的行 origin 仍是模板坐标，因此绝不能拿它去重，否则整块明细都会丢。
        cells = {}
        for c in result["cells"]:
            cells["%d,%d" % (c["row"], c["col"])] = c["value"]["value"]

        resp["sheet"] = {
            "rows": result.get("rows") or tpl["grid"]["rows"],
            "cols": result.get("cols") or tpl["grid"]["cols"],
            "cells": cells,
            "merges": [m for m in (result.get("merges") or [])],
            "col_widths": result.get("col_widths"),
            "pagination": result.get("pagination"),
        }
        INTERNAL = {"result.json", "workbook.json"}
        for name in sorted(os.listdir(art)):
            if name in INTERNAL:
                continue
            fp = os.path.join(art, name)
            resp["files"].append({"name": name, "size": os.path.getsize(fp),
                                  "ext": os.path.splitext(name)[1].lstrip(".")})
        resp["template_name"] = "template.json"
        return self._json(resp)


def _role_label(role):
    return {"group": "分组列", "detail": "明细列", "measure": "数值列"}.get(role, "未定")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8777)
    ap.add_argument("--host", default="127.0.0.1")
    args = ap.parse_args()
    os.makedirs(WORK, exist_ok=True)
    # 只留最近 30 次会话，避免 work/ 无限膨胀
    try:
        ds = [(os.path.getmtime(os.path.join(WORK, d)), d)
              for d in os.listdir(WORK) if TOKEN_RE.match(d)]
        for _, d in sorted(ds)[:-30]:
            shutil.rmtree(os.path.join(WORK, d), ignore_errors=True)
    except OSError:
        pass

    if not os.path.isfile(RPX):
        print("!! 找不到 rpx 可执行文件：%s" % RPX)
        print("   用 RPX_BIN 环境变量指定，例如：")
        print("   RPX_BIN=F:\\path\\rpx.exe python server.py")

    srv = ThreadingHTTPServer((args.host, args.port), Handler)
    print("RPX 报表工作台已启动 → http://%s:%d" % (args.host, args.port))
    print("引擎：%s" % RPX)
    print("按 Ctrl+C 停止\n")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        print("\n已停止")
        srv.server_close()


if __name__ == "__main__":
    main()
