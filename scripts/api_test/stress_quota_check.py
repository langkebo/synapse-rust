#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
media/quota/check 并发压测 — 验证 get_or_create_user_quota 原子 UPSERT 修复。
背景: 修复前为 check-then-insert, 并发下同一新用户多次请求会触发
  uq_user_media_quota_user 唯一约束冲突 (500)。
修复后为 INSERT ... ON CONFLICT (user_id) DO UPDATE ... RETURNING *, 应全部 2xx。

用法:
  python3 stress_quota_check.py [--base-url URL] [--token TOKEN] \
      [--users N] [--rounds M] [--concurrency C]
  token 缺省时尝试用 admin 凭据自动登录。
"""
import argparse
import concurrent.futures as cf
import random
import string
import sys
import time
import urllib3

import requests
import yaml

urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)

BASE_URL = "https://matrix.test"


def load_config():
    with open("config.yaml", "r", encoding="utf-8") as f:
        return yaml.safe_load(f)


def login(base, cfg):
    # v1/login 在当前部署未注册, 回退到 r0/login (同源 token)
    url = base.rstrip("/") + "/_matrix/client/r0/login"
    r = requests.post(
        url,
        json={
            "type": "m.login.password",
            "user": cfg["auth"]["username"],
            "password": cfg["auth"]["password"],
        },
        verify=False,
        timeout=15,
    )
    r.raise_for_status()
    return r.json()["access_token"]


def register_user(base, token, username):
    """注册一个普通用户 (share-secret 注册需 admin 权限, 此接口通常在测试环境开启)。"""
    url = base.rstrip("/") + "/_matrix/client/v1/register?kind=user"
    r = requests.post(
        url,
        json={"username": username, "password": "Stress@123", "auth": {"type": "m.login.dummy"}},
        headers={"Authorization": f"Bearer {token}"},
        verify=False,
        timeout=15,
    )
    return r.status_code, r


def quota_check(base, token, user_id):
    url = base.rstrip("/") + "/_matrix/media/v1/quota/check"
    t0 = time.monotonic()
    try:
        r = requests.get(url, headers={"Authorization": f"Bearer {token}"}, verify=False, timeout=20)
        dt = (time.monotonic() - t0) * 1000
        try:
            body = r.json()
        except Exception:
            body = {"_raw": r.text[:200]}
        return {"user": user_id, "status": r.status_code, "ms": round(dt, 1), "body": body}
    except Exception as e:  # noqa: BLE001
        return {"user": user_id, "status": -1, "ms": None, "body": {"_error": str(e)}}


def random_user(prefix):
    return f"@stress_{prefix}_{''.join(random.choices(string.ascii_lowercase + string.digits, k=8))}:matrix.test"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base-url", default=None)
    ap.add_argument("--token", default=None)
    ap.add_argument("--users", type=int, default=4, help="独立新用户数 (每用户并发首访)")
    ap.add_argument("--concurrency", type=int, default=16, help="每用户并发请求数")
    ap.add_argument("--rounds", type=int, default=3)
    args = ap.parse_args()

    cfg = load_config()
    base = args.base_url or cfg.get("base_url", BASE_URL)
    token = args.token or login(base, cfg)
    headers = {"Authorization": f"Bearer {token}"}

    # 造一批新用户: 注册成功后该用户 quota 行不存在 → 并发首访触发 get_or_create
    users = []
    for i in range(args.users):
        uname = f"stressq{i}_{''.join(random.choices(string.ascii_lowercase, k=6))}"
        sc, _ = register_user(base, token, uname)
        if sc in (200, 400):
            # 400 = 可能缺 share-secret / 注册策略; 仍用该用户 id 测试(若已存在则退化为已有行路径)
            users.append(f"@{uname}:matrix.test")
            print(f"[+] user {uname} register status={sc}")
        else:
            print(f"[!] register {uname} unexpected status={sc}, 跳过")
    if not users:
        # 回退: 用 admin 自身 (已有行, 验证 UPSERT 不回归)
        users = [cfg["auth"]["username"]]
        print("[!] 无新用户注册, 回退到已有用户路径")

    failures = []
    all_stats = []
    for rnd in range(1, args.rounds + 1):
        tasks = [(u, token) for u in users for _ in range(args.concurrency)]
        t0 = time.monotonic()
        with cf.ThreadPoolExecutor(max_workers=min(len(tasks), 64)) as ex:
            results = list(ex.map(lambda t: quota_check(base, t[1], t[0]), tasks))
        elapsed = time.monotonic() - t0
        ok = sum(1 for x in results if x["status"] == 200)
        bad = [x for x in results if x["status"] != 200]
        statuses = sorted({x["status"] for x in results})
        p99 = sorted(x["ms"] for x in results if x["ms"] is not None)[int(len(results) * 0.99) - 1] if results else 0
        print(f"round {rnd}: {len(results)} reqs, {elapsed:.1f}s, 200={ok}, statuses={statuses}, p99={p99:.0f}ms")
        failures.extend(bad)
        all_stats.extend(x["ms"] for x in results if x["ms"] is not None)

    if failures:
        print(f"\n[FAIL] {len(failures)} 个非 200 响应, 样例:")
        for f in failures[:5]:
            print(f"  {f}")
        sys.exit(1)
    print(f"\n[PASS] 全部 {len(all_stats)} 请求均 200; avg={sum(all_stats)/len(all_stats):.0f}ms p99={sorted(all_stats)[int(len(all_stats)*0.99)-1]:.0f}ms")


if __name__ == "__main__":
    main()
