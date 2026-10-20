# -*- coding: utf-8 -*-
"""以脱离进程组的方式常驻三个本地服务（关闭工具调用后仍存活）。

- admin 后端 cms-server     127.0.0.1:8088  (cwd=server, DATABASE_URL 默认 ./cms.db)
- admin 前端 Vite           127.0.0.1:5188
- 公开主页 coucouya Vite     127.0.0.1:5199

Windows: DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP，stdout/stderr 落日志文件。
"""
import os
import socket
import subprocess
import time

BASE = r"F:\project\admin\docs\report-engine-promo"
NODE = r"C:\Users\Administrator\.workbuddy\binaries\node\versions\22.22.2-6\node.exe"
DETACHED = subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP


def up(port: int) -> bool:
    s = socket.socket()
    s.settimeout(0.4)
    try:
        return s.connect_ex(("127.0.0.1", port)) == 0
    finally:
        s.close()


JOBS = [
    # name, exe, args, cwd, extra env, log, port
    ("cms", r"F:\project\admin\server\tgt_build4\debug\cms-server.exe", [],
     r"F:\project\admin\server", {"PORT": "8088"},
     os.path.join(BASE, "_serve_cms.log"), 8088),
    ("admin", NODE, [r"node_modules\vite\bin\vite.js", "--host", "127.0.0.1", "--port", "5188"],
     r"F:\project\admin\web", {},
     os.path.join(BASE, "_serve_admin.log"), 5188),
    ("coucouya", NODE, [r"node_modules\vite\bin\vite.js", "--host", "127.0.0.1", "--port", "5199"],
     r"F:\project\coucouya", {},
     os.path.join(BASE, "_serve_coucouya.log"), 5199),
]


def main() -> None:
    for name, exe, args, cwd, extra, log, port in JOBS:
        if up(port):
            print(f"{name}: 已在 {port} 运行，跳过")
            continue
        env = dict(os.environ)
        env.update(extra)
        f = open(log, "ab")
        p = subprocess.Popen(
            [exe] + args, cwd=cwd, env=env,
            stdout=f, stderr=f, stdin=subprocess.DEVNULL,
            creationflags=DETACHED, close_fds=True,
        )
        print(f"{name}: 已启动 pid={p.pid} port={port}")

    for _ in range(45):
        if all(up(j[6]) for j in JOBS):
            break
        time.sleep(1)

    for name, _exe, _args, _cwd, _extra, _log, port in JOBS:
        print(f"{name} {port}: {'UP' if up(port) else 'DOWN'}")


if __name__ == "__main__":
    main()
