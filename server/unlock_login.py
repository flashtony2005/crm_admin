# -*- coding: utf-8 -*-
"""清除登录锁定记录（login_locks 表）。

出现「尝试次数过多，请 15 分钟后再试」时运行本脚本，即可立即重新登录。
只删除锁定 / 失败计数行，不触碰 users、文章、订单等任何业务数据。

用法：python unlock_login.py
配套：unlock_login.cmd（双击即可）
"""

import os
import sqlite3
import sys

DB = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cms.db")


def main() -> int:
    if not os.path.exists(DB):
        print("[unlock] database not found: %s" % DB)
        return 1

    conn = sqlite3.connect(DB, timeout=15)
    try:
        rows = conn.execute(
            "SELECT username, fails, locked_until FROM login_locks"
        ).fetchall()

        if rows:
            print("[unlock] lock records found:")
            for username, fails, locked_until in rows:
                print(
                    "         %-14s fails=%s  locked_until=%s"
                    % (username, fails, locked_until or "-")
                )
        else:
            print("[unlock] no lock records - nobody is locked right now")

        conn.execute("DELETE FROM login_locks")
        conn.commit()
        print("[unlock] cleared %d record(s); you can log in immediately now." % len(rows))
        print("[unlock] accounts: owner / editor / viewer   password: demo1234")
    except sqlite3.OperationalError as exc:
        print("[unlock] failed (table login_locks may not exist yet): %s" % exc)
        return 1
    finally:
        conn.close()

    return 0


if __name__ == "__main__":
    sys.exit(main())
