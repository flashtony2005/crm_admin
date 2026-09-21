#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
turso_local.py — 本地 Turso 协议服务（无 Docker / WSL 依赖）

实现 libsql HTTP /v2/pipeline 协议的服务端子集，与 server/src/cmsdb.rs 的
Turso 客户端严格对齐（字段级兼容），底层直接读写现有 SQLite 文件（默认
./cms.db）。用途：让 cms-server 以 TURSO_URL=http://127.0.0.1:8081 全量跑在
Turso 代码路径上，本地零容器依赖；将来切换真 Turso 云只需改 TURSO_URL。

协议要点（对齐 cmsdb.rs 客户端的实际解析行为）：
- 请求：{"requests":[{"type":"execute","stmt":{"sql":...,"args":[{"type":...,"value":...}]}}], "transaction":"read"|"write"}
- 响应：{"results":[{"type":"ok","response":{"type":"execute","result":{
          "cols":[{"name":...}],"rows":[[...]],
          "affected_row_count":N,"rows_written":N,"last_insert_rowid":N}}}]
        出错条目为 {"type":"error","error":"..."}（客户端逐条检查 /error）
- 事务：客户端 tx 路径在单次 pipeline 内发 BEGIN IMMEDIATE / 业务 / COMMIT；
  任一语句失败 → ROLLBACK + 该条及后续标 error（与真 libsql 语义一致）

运行：python turso_local.py [port]   默认 127.0.0.1:8081，库文件 ./cms.db
"""
import base64
import json
import os
import sqlite3
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DB_PATH = os.environ.get("TURSO_LOCAL_DB", os.path.join(os.path.dirname(os.path.abspath(__file__)), "cms.db"))
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else int(os.environ.get("TURSO_LOCAL_PORT", "8081"))

conn = sqlite3.connect(DB_PATH, check_same_thread=False, timeout=10)
conn.isolation_level = None  # autocommit；事务由显式 BEGIN/COMMIT 控制
conn.row_factory = None
lock = threading.Lock()


def to_py(v):
    """请求参数 {type,value} → Python 值"""
    if not isinstance(v, dict):
        return v
    t, val = v.get("type"), v.get("value")
    if t == "integer":
        return int(val)
    if t == "real":
        return float(val)
    if t == "text":
        return str(val)
    if t == "blob":
        return base64.b64decode(val) if isinstance(val, str) else val
    return None  # null / 未知类型


def to_jsonable(v):
    """行值 → JSON 可编码（BLOB 转 base64 文本，应用当前无 BLOB 列）"""
    if isinstance(v, bytes):
        return base64.b64encode(v).decode()
    return v


def execute_stmt(sql, args):
    cur = conn.execute(sql, args)
    cols = [d[0] for d in cur.description] if cur.description else []
    rows = [[to_jsonable(c) for c in r] for r in cur.fetchall()] if cols else []
    result = {
        "cols": [{"name": n} for n in cols],
        "rows": rows,
        "affected_row_count": max(cur.rowcount, 0),
        "rows_written": max(cur.rowcount, 0),
    }
    if cur.lastrowid:
        result["last_insert_rowid"] = cur.lastrowid
    return result


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):  # 安静模式
        pass

    def _json(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path in ("/", "/health", "/healthz"):
            self._json(200, {"ok": True, "db": DB_PATH, "protocol": "libsql/v2-pipeline-local"})
        else:
            self._json(404, {"error": "not found"})

    def do_POST(self):
        if self.path != "/v2/pipeline":
            self._json(404, {"error": "not found"})
            return
        try:
            n = int(self.headers.get("Content-Length", "0"))
            body = json.loads(self.rfile.read(n) or b"{}")
        except Exception as e:
            self._json(400, {"error": f"bad request: {e}"})
            return

        reqs = body.get("requests") or []
        results = []
        with lock:
            failed_at = None
            for i, req in enumerate(reqs):
                if failed_at is not None:
                    results.append({"type": "error", "error": "pipeline aborted by earlier error"})
                    continue
                if req.get("type") != "execute":
                    results.append({"type": "error", "error": f"unsupported request type: {req.get('type')}"})
                    failed_at = i
                    continue
                stmt = req.get("stmt") or {}
                sql = stmt.get("sql") or ""
                args = [to_py(a) for a in (stmt.get("args") or [])]
                try:
                    result = execute_stmt(sql, args)
                    results.append({"type": "ok", "response": {"type": "execute", "result": result}})
                except Exception as e:
                    try:
                        if conn.in_transaction:
                            conn.execute("ROLLBACK")
                    except Exception:
                        pass
                    results.append({"type": "error", "error": str(e)})
                    failed_at = i

        self._json(200, {"results": results})


def main():
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA busy_timeout=5000")
    conn.execute("PRAGMA foreign_keys=ON")
    print(f"[turso-local] db   = {DB_PATH}", flush=True)
    print(f"[turso-local] listening on http://127.0.0.1:{PORT}  (POST /v2/pipeline)", flush=True)
    srv = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
