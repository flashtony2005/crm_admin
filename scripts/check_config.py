#!/usr/bin/env python3
"""coucouya CMS — 配置检查工具（部署前门禁）。

解析 .env（或 .env.example）并校验关键环境变量，输出分级报告：
  OK   配置正确
  INFO 可选功能未开启（进入测试/降级模式）
  WARN 可用但有隐患（占位符、http、密钥环境不匹配等）
  FAIL 必填缺失或格式错误（会导致启动失败或安全/功能问题）

退出码：存在 FAIL → 1（--strict 时 WARN 也计为失败 → 1）；否则 0。
仅使用 Python 标准库，跨平台（Windows / Linux / macOS）。

示例：
  python3 scripts/check_config.py --env-file .env --strict
  python3 scripts/check_config.py --env-file .env.example
  python3 scripts/check_config.py --json
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# 占位符特征（出现即视为未真正填写）
PLACEHOLDER = (
    "your-",
    "xxx",
    "replace",
    "change-me",
    "changeme",
    "placeholder",
    "<",
    ">",
    "todo",
)

_LEVEL_ORDER = {"OK": 0, "INFO": 1, "WARN": 2, "FAIL": 3}
_LEVEL_COLOR = {
    "OK": "\033[32m",
    "INFO": "\033[36m",
    "WARN": "\033[33m",
    "FAIL": "\033[31m",
}
_RESET = "\033[0m"


def is_placeholder(v: str) -> bool:
    return any(t in v.lower() for t in PLACEHOLDER)


def load_env(path: Path) -> dict[str, str]:
    """解析 KEY=VALUE，去引号，跳过注释/空行。对 BOM / CRLF 鲁棒。"""
    values: dict[str, str] = {}
    if not path.exists():
        return values
    # utf-8-sig 自动剥离 BOM；逐行 rstrip('\r') 兼容 Windows CRLF
    text = path.read_text(encoding="utf-8-sig")
    for raw in text.splitlines():
        line = raw.strip().rstrip("\r")
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[len("export "):].strip()
        m = re.match(r'^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$', line)
        if not m:
            continue
        key, val = m.group(1), m.group(2).strip()
        if (val.startswith('"') and val.endswith('"')) or (
            val.startswith("'") and val.endswith("'")
        ):
            val = val[1:-1]
        values[key] = val
    return values


def validate(values: dict[str, str], profile: str) -> list[tuple[str, str, str]]:
    """返回 (变量, 级别, 信息) 列表。"""
    issues: list[tuple[str, str, str]] = []
    get = lambda k: values.get(k, "")
    envv = (get("CMS_ENV") or "development").strip()

    if envv not in ("development", "production", "test"):
        issues.append(("CMS_ENV", "FAIL", "取值应为 development | production | test"))

    # PORT
    port = get("PORT") or "8088"
    if not port.isdigit() or not (1 <= int(port) <= 65535):
        issues.append(("PORT", "FAIL", f"应为 1-65535 的整数，当前：{port}"))

    # JWT_SECRET
    jwt = get("JWT_SECRET")
    if not jwt:
        if profile == "production":
            issues.append(("JWT_SECRET", "FAIL", "生产环境必须设置 JWT_SECRET（启动会被拒绝）"))
        else:
            issues.append(("JWT_SECRET", "WARN", "未设置，将使用内置弱默认值（仅限开发）"))
    elif jwt == "dev-secret-change-me":
        issues.append(("JWT_SECRET", "FAIL", "不得使用内置默认值 dev-secret-change-me"))
    elif is_placeholder(jwt):
        issues.append(("JWT_SECRET", "WARN", "值疑似占位符，请替换为高强度随机串"))
    elif len(jwt) < 16:
        issues.append(("JWT_SECRET", "FAIL", "长度至少 16 位"))

    # PUBLIC_BASE_URL
    base = get("PUBLIC_BASE_URL")
    if not base:
        issues.append(("PUBLIC_BASE_URL", "WARN", "未设置，OAuth/Stripe/SEO 回跳将使用 localhost 默认值"))
    else:
        if not re.match(r"^https?://[^/\s]+\S*$", base):
            issues.append(("PUBLIC_BASE_URL", "FAIL", f"格式应为 http(s)://host，当前：{base}"))
        elif base.startswith("http://") and profile == "production":
            issues.append(("PUBLIC_BASE_URL", "WARN", "生产环境建议使用 https://"))

    # 数据库 / Turso
    if get("TURSO_URL"):
        if not get("TURSO_AUTH_TOKEN"):
            issues.append(("TURSO_AUTH_TOKEN", "FAIL", "设置了 TURSO_URL 必须同时设置 TURSO_AUTH_TOKEN"))
    else:
        db = get("DATABASE_URL") or "sqlite:./cms.db?mode=rwc"
        if not db:
            issues.append(("DATABASE_URL", "FAIL", "DATABASE_URL 不能为空"))

    # Stripe
    sk = get("STRIPE_SECRET_KEY")
    if sk:
        if not (sk.startswith("sk_live_") or sk.startswith("sk_test_")):
            issues.append(("STRIPE_SECRET_KEY", "FAIL", "应以 sk_live_ 或 sk_test_ 开头"))
        elif is_placeholder(sk):
            issues.append(("STRIPE_SECRET_KEY", "WARN", "值疑似占位符，请替换为真实密钥"))
        elif sk.startswith("sk_live_") and profile != "production":
            issues.append(("STRIPE_SECRET_KEY", "WARN", "生产密钥 sk_live_ 用于非生产环境"))
        elif sk.startswith("sk_test_") and profile == "production":
            issues.append(("STRIPE_SECRET_KEY", "WARN", "测试密钥 sk_test_ 用于生产环境，不会真实扣费"))
        wh = get("STRIPE_WEBHOOK_SECRET")
        if not wh:
            issues.append(("STRIPE_WEBHOOK_SECRET", "WARN", "已配置密钥但未设置 Webhook 密钥，Stripe 回调签名将不校验"))
        elif is_placeholder(wh):
            issues.append(("STRIPE_WEBHOOK_SECRET", "WARN", "值疑似占位符"))
    else:
        issues.append(("STRIPE_SECRET_KEY", "INFO", "未配置 → 订阅进入测试模式（不真实扣费）"))

    # SMTP
    smtp_keys = ["SMTP_HOST", "SMTP_PORT", "SMTP_USER", "SMTP_PASS", "SMTP_FROM"]
    if any(get(k) for k in smtp_keys):
        if not get("SMTP_HOST"):
            issues.append(("SMTP_HOST", "FAIL", "配置了 SMTP 相关变量但缺少 SMTP_HOST"))
        portv = get("SMTP_PORT") or "587"
        if not portv.isdigit():
            issues.append(("SMTP_PORT", "FAIL", "SMTP_PORT 应为整数"))
        frm = get("SMTP_FROM")
        if frm and not re.match(r"^[^@\s]+@[^@\s]+\.[^@\s]+$", frm):
            issues.append(("SMTP_FROM", "WARN", f"发件人邮箱格式可疑：{frm}"))
        if is_placeholder(get("SMTP_PASS")) or is_placeholder(get("SMTP_USER")):
            issues.append(("SMTP_*", "WARN", "SMTP 凭据疑似占位符"))
    else:
        issues.append(("SMTP_*", "INFO", "未配置 → 邮件发送进入测试模式（不真实发送）"))

    # AI（可选）
    if get("LLM_API_KEY") and is_placeholder(get("LLM_API_KEY")):
        issues.append(("LLM_API_KEY", "WARN", "值疑似占位符"))

    # 评论审核开关
    caa = get("COMMENTS_AUTO_APPROVE")
    if caa and caa.lower() not in ("0", "1", "true", "false"):
        issues.append(("COMMENTS_AUTO_APPROVE", "WARN", "应为 0 / 1 / true / false"))

    return issues


def colorize(level: str, text: str, use_color: bool) -> str:
    if not use_color:
        return text
    return f"{_LEVEL_COLOR.get(level, '')}{text}{_RESET}"


def main() -> int:
    ap = argparse.ArgumentParser(description="coucouya CMS 配置检查工具")
    ap.add_argument("--env-file", default=None, help="指定 .env 路径（默认：存在 .env 则用 .env，否则 .env.example）")
    ap.add_argument("--strict", action="store_true", help="WARN 也计为失败（部署门禁用）")
    ap.add_argument("--json", action="store_true", help="输出 JSON")
    ap.add_argument("--quiet", action="store_true", help="仅输出 FAIL/WARN")
    args = ap.parse_args()

    here = Path(__file__).resolve().parent.parent
    env_file = Path(args.env_file) if args.env_file else (
        here / ".env" if (here / ".env").exists() else here / ".env.example"
    )
    values = load_env(env_file)
    profile = (values.get("CMS_ENV") or "development").strip()
    if profile not in ("development", "production", "test"):
        profile = "development"

    issues = validate(values, profile)
    use_color = sys.stdout.isatty() and not args.json

    if args.json:
        out = {
            "env_file": str(env_file),
            "profile": profile,
            "issues": [
                {"key": k, "level": lvl, "message": msg} for k, lvl, msg in issues
            ],
        }
        print(json.dumps(out, ensure_ascii=False, indent=2))
    else:
        print(f"配置检查：{env_file}  (profile={profile})\n")
        key_w = max([len(k) for k, _, _ in issues] + [8])
        counts = {lvl: 0 for lvl in _LEVEL_ORDER}
        shown = 0
        for k, lvl, msg in sorted(issues, key=lambda x: -_LEVEL_ORDER[x[1]]):
            counts[lvl] += 1
            if args.quiet and lvl in ("OK", "INFO"):
                continue
            shown += 1
            print(f"  {colorize(lvl, lvl.ljust(4), use_color)}  {k.ljust(key_w)}  {msg}")
        if shown == 0:
            print("  （无条目）")
        print(
            f"\n汇总：FAIL={counts['FAIL']}  WARN={counts['WARN']}  "
            f"INFO={counts['INFO']}  OK={counts['OK']}"
        )

    has_fail = any(lvl == "FAIL" for _, lvl, _ in issues)
    has_warn = any(lvl == "WARN" for _, lvl, _ in issues)
    if has_fail or (args.strict and has_warn):
        print("\n✗ 配置检查未通过。" + ("（strict 模式：WARN 亦视为失败）" if args.strict and has_warn else ""))
        return 1
    print("\n✓ 配置检查通过。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
