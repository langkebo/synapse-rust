#!/usr/bin/env python3
"""逐一测试 synapse-rust 的联邦 (Server-Server) 功能端点。

用法:
    /tmp/peer-synapse/bin/python scripts/federation-test/test_federation_features.py

前置条件:
    * 双实例栈 (synapse-a / synapse-b) 已启动且健康。
    * 已运行过 scripts/federation-test/test_federation.sh（产生可复用的用户）。

原理:
    protected 联邦端点需要 X-Matrix Authorization 签名。签名密钥是各实例
    key_rotation_manager 存于 DB `federation_signing_keys` 的 current key（AES-256-GCM
    加密，master key 来自 FEDERATION_MASTER_KEY）。本脚本从 DB 读出并解密后，
    用 canonical JSON + ed25519 对请求签名，再逐一请求每个端点。

    额外做一次真实跨服邀请（客户端 API 触发 A → B 的 /v2/invite 出站联邦调用），
    用于验证入站邀请处理链路。

覆盖率:
    `_synapse/federation/*` 为非标准扩展，nginx 联邦端口 (18448/18449) 有意不代理，
    因此这些端点改为经应用端口 (18008/18009) 直连验证 handler 行为。
"""
from __future__ import annotations

import base64
import json
import os
import ssl
import subprocess
import time
import urllib.parse
import urllib.request

from canonicaljson import encode_canonical_json
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.kdf.hkdf import HKDF

INFO = b"synapse-rust-signing-key-encryption-v1"

# ── 测试实例配置（与 docker/federation-test/.env.* 一致）──────────────────
INSTANCES = {
    "A": {
        "server_name": "synapse-a.federation.test",
        "master_key": "357eda58584fe6c0a6a2feba7ea418eca1c500937888f51d779c2a8a52941b45",
        "db_container": "synapse-federation-a-db",
        "db_name": "synapse_a",
        "db_user": "synapse",
        "db_password": "synapse_a_pwd",
        "tls_port": 18448,
        "app_port": 18008,
        "user": "@user_a:synapse-a.federation.test",
        "password": "FedTest@123",
    },
    "B": {
        "server_name": "synapse-b.federation.test",
        "master_key": "6ed24177307bad9efbb16e57fd0fbedf063a79d034c5a2d2cd1ba20c299497a6",
        "db_container": "synapse-federation-b-db",
        "db_name": "synapse_b",
        "db_user": "synapse",
        "db_password": "synapse_b_pwd",
        "tls_port": 18449,
        "app_port": 18009,
        "user": "@user_b:synapse-b.federation.test",
        "password": "FedTest@456",
    },
}

USER_A = INSTANCES["A"]["user"]
USER_B = INSTANCES["B"]["user"]


def q(s: str) -> str:
    """URL-encode a path component."""
    return urllib.parse.quote(s, safe="")


def psql(inst: dict, sql: str) -> str:
    return subprocess.run(
        ["docker", "exec", inst["db_container"], "psql", "-U", inst["db_user"],
         "-d", inst["db_name"], "-tAc", sql],
        capture_output=True, text=True, check=True,
    ).stdout.strip()


# ── 密钥解密（HKDF-SHA256 + AES-256-GCM）────────────────────────────────────
def derive_key(master_key_hex: str) -> bytes:
    hkdf = HKDF(algorithm=hashes.SHA256(), length=32, salt=None, info=INFO)
    return hkdf.derive(master_key_hex.encode("utf-8"))


def decrypt_secret(enc_secret: str, master_key_hex: str) -> str:
    combined = base64.b64decode(enc_secret.removeprefix("enc:"))
    nonce, ct = combined[:12], combined[12:]
    return AESGCM(derive_key(master_key_hex)).decrypt(nonce, ct, None).decode("utf-8")


def fetch_signing_key(inst: dict) -> tuple[str, bytes]:
    out = psql(inst, "SELECT key_id, secret_key FROM federation_signing_keys "
                     "ORDER BY created_ts DESC LIMIT 1")
    key_id, enc_secret = out.split("|", 1)
    seed_b64 = decrypt_secret(enc_secret, inst["master_key"])
    seed = base64.b64decode(seed_b64 + "=" * (-len(seed_b64) % 4))
    return key_id, seed


def sign_request(origin_inst: dict, key_id: str, seed: bytes,
                 method: str, path: str, destination: str,
                 content: dict | None) -> str:
    obj = {"method": method, "uri": path, "origin": origin_inst["server_name"],
           "destination": destination}
    if content is not None:
        obj["content"] = content
    canonical = encode_canonical_json(obj)
    sig = Ed25519PrivateKey.from_private_bytes(seed).sign(canonical)
    sig_b64 = base64.b64encode(sig).decode().rstrip("=")
    # `ts` rides in the header only, never in the signed object: the spec's
    # signed JSON is exactly {method, uri, origin, destination, content} and
    # `ts` is an ordinary (ignored-if-unknown) authorisation parameter.  It
    # therefore does NOT make the signature unique — see `cache_busted`.
    ts_ms = int(time.time() * 1000)
    return (
        f'X-Matrix origin="{origin_inst["server_name"]}",'
        f'destination="{destination}",key="{key_id}",sig="{sig_b64}",ts="{ts_ms}"'
    )


# A per-process nonce, mixed into every signed request URI (`cache_busted`).
_RUN_NONCE = os.urandom(6).hex()
_nonce_counter = 0


def cache_busted(path: str) -> str:
    """Append an opaque, per-request query parameter to a signed request path.

    The server signs `path_and_query` and keys its replay-protection cache on
    the signature hash, so a probe carrying an identical signature is rejected
    with 401 for the whole 300 s window — which made every re-run of this script
    fail on the protected endpoints even though each run is independent.  Real
    federation traffic never repeats a signature (every transaction carries a
    fresh txn id / event id); a replayed probe needs the same treatment.  The
    handlers ignore unknown query parameters.
    """
    global _nonce_counter
    _nonce_counter += 1
    sep = "&" if "?" in path else "?"
    return f"{path}{sep}_nocache={_RUN_NONCE}-{_nonce_counter}"


def _http(method: str, url: str, headers: dict, body: dict | None,
          ctx: ssl.SSLContext) -> tuple[int, str]:
    data = None if body is None else json.dumps(body, separators=(",", ":")).encode("utf-8")
    req = urllib.request.Request(url, data=data, method=method, headers=headers)
    try:
        with urllib.request.urlopen(req, context=ctx, timeout=15) as resp:
            return resp.status, resp.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode("utf-8", "replace")
    except Exception as e:  # noqa: BLE001
        return 0, f"<transport error: {e}>"


_TLS_CTX = ssl.create_default_context()
_TLS_CTX.check_hostname = False
_TLS_CTX.verify_mode = ssl.CERT_NONE
_PLAIN_CTX = ssl.create_default_context()  # unused for http, kept for symmetry


def federated_request(target: str, method: str, path: str,
                      body: dict | None = None) -> tuple[int, str]:
    """经联邦 TLS 端口（nginx 边车）发起已签名请求。签名 origin 为对端实例。"""
    tgt = INSTANCES[target]
    origin = INSTANCES["B" if target == "A" else "A"]
    key_id, seed = fetch_signing_key(origin)
    path = cache_busted(path)
    auth = sign_request(origin, key_id, seed, method, path, tgt["server_name"], body)
    url = f"https://localhost:{tgt['tls_port']}{path}"
    return _http(method, url, {"Authorization": auth, "Content-Type": "application/json"},
                 body, _TLS_CTX)


def local_request(target: str, method: str, path: str,
                  body: dict | None = None) -> tuple[int, str]:
    """经应用端口（绕过 nginx）发起已签名请求，用于验证 `_synapse/*` 等未在联邦端口
    白名单中的扩展端点 handler。"""
    tgt = INSTANCES[target]
    origin = INSTANCES["B" if target == "A" else "A"]
    key_id, seed = fetch_signing_key(origin)
    path = cache_busted(path)
    auth = sign_request(origin, key_id, seed, method, path, tgt["server_name"], body)
    url = f"http://localhost:{tgt['app_port']}{path}"
    return _http(method, url, {"Authorization": auth, "Content-Type": "application/json"},
                 body, _PLAIN_CTX)


def public_request(target: str, method: str, path: str,
                   body: dict | None = None) -> tuple[int, str]:
    tgt = INSTANCES[target]
    url = f"https://localhost:{tgt['tls_port']}{path}"
    return _http(method, url, {"Content-Type": "application/json"}, body, _TLS_CTX)


def verdict(code: int, expect: tuple[int, ...] = ()) -> str:
    if expect and code in expect:
        return "EXPECTED"
    if 200 <= code < 300:
        return "PASS"
    if code == 401:
        return "AUTH_FAIL"
    if code == 500:
        return "SERVER_ERROR"
    if code == 404:
        return "NOT_FOUND"
    if code == 403:
        return "FORBIDDEN"
    if code == 400:
        return "BAD_REQUEST"
    if code == 405:
        return "METHOD_NA"
    if code == 0:
        return "TRANSPORT"
    return f"HTTP_{code}"


# ── 真实跨服邀请（客户端 API 触发 A → B 出站联邦 /v2/invite）─────────────────
def client_login(inst: dict) -> str | None:
    body = {"type": "m.login.password",
            "identifier": {"type": "m.id.user", "user": inst["user"]},
            "password": inst["password"]}
    code, text = _http("POST", f"http://localhost:{inst['app_port']}/_matrix/client/v3/login",
                       {"Content-Type": "application/json"}, body, _PLAIN_CTX)
    if code != 200:
        print(f"  [warn] login as {inst['user']} failed: {code} {text[:160]}")
        return None
    return json.loads(text).get("access_token")


def client_api(inst: dict, method: str, path: str, token: str, body: dict | None):
    return _http(method, f"http://localhost:{inst['app_port']}{path}",
                 {"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
                 body, _PLAIN_CTX)


def run_real_invite() -> dict:
    """A 创建私有房间并邀请 @user_b，验证邀请经联邦送达 B。返回真实事件/房间 ID。"""
    print("== 0. 真实跨服邀请（客户端 API 触发出站联邦 /v2/invite）==")
    result = {"ok": False, "room_id": None, "invite_event_id": None}
    token_a = client_login(INSTANCES["A"])
    if not token_a:
        print("  [SKIP] 无法登录 user_a")
        return result

    # 房间名带时间戳，保证脚本可重复运行（否则同名房间触发 409 M_ROOM_IN_USE）。
    code, text = client_api(INSTANCES["A"], "POST", "/_matrix/client/v3/createRoom",
                            token_a, {"visibility": "private", "preset": "private_chat",
                                      "name": f"Federation Invite Test {int(time.time())}"})
    if code != 200:
        print(f"  [FAIL] createRoom: {code} {text[:200]}")
        return result
    room_id = json.loads(text)["room_id"]
    result["room_id"] = room_id
    print(f"  [ok] A 创建房间 {room_id}")

    code, text = client_api(INSTANCES["A"], "POST",
                            f"/_matrix/client/v3/rooms/{q(room_id)}/invite",
                            token_a, {"user_id": USER_B})
    if code != 200:
        print(f"  [FAIL] invite {USER_B}: {code} {text[:200]}")
        return result
    print(f"  [ok] A 邀请 {USER_B} 成功（客户端 API 200）")

    # 从 A 的 DB 取这次邀请事件的 event_id
    # 注意：events 表列名为 event_type（非 type），且无 membership 列，
    # membership 存在 content->>'membership'。
    row = psql(INSTANCES["A"],
               "SELECT event_id FROM events WHERE room_id = '%s' "
               "AND event_type = 'm.room.member' AND state_key = '%s' "
               "AND content->>'membership' = 'invite' "
               "ORDER BY origin_server_ts DESC LIMIT 1"
               % (room_id.replace("'", "''"), USER_B.replace("'", "''")))
    if row:
        result["invite_event_id"] = row.splitlines()[0]
        print(f"  [ok] 邀请事件 {result['invite_event_id']}")

    # 验证 B 侧收到 invite（联邦入站 /v2/invite 生效）
    b_row = psql(INSTANCES["B"],
                 "SELECT membership FROM room_memberships WHERE room_id = '%s' "
                 "AND user_id = '%s' ORDER BY joined_ts DESC LIMIT 1"
                 % (room_id.replace("'", "''"), USER_B.replace("'", "''")))
    if b_row.strip() == "invite":
        result["ok"] = True
        print("  [PASS] B 侧 room_memberships 显示 invite —— 联邦邀请链路打通")
    else:
        print(f"  [FAIL] B 侧未观察到 invite（membership={b_row!r}）")
    return result


def main() -> int:
    invite = run_real_invite()
    print()

    room_q = q(invite["room_id"]) if invite["room_id"] else q("!nonexistent:synapse-a.federation.test")
    ev_q = q(invite["invite_event_id"]) if invite["invite_event_id"] else q("$nonexistent")
    user_b_q = q(USER_B)
    user_a_q = q(USER_A)
    server_a_q = q(INSTANCES["A"]["server_name"])
    server_b_q = q(INSTANCES["B"]["server_name"])
    key_b = fetch_signing_key(INSTANCES["B"])[0]

    results = []

    def record(category, endpoint, method, target, path, body=None,
               public=False, via="fed", expect=()):
        if public:
            code, text = public_request(target, method, path, body)
        elif via == "app":
            code, text = local_request(target, method, path, body)
        else:
            code, text = federated_request(target, method, path, body)
        results.append((category, endpoint, method, verdict(code, expect), code,
                        text[:150].replace("\n", " ")))
        return code, text

    # ── 1. Server keys / discovery（public，无签名）─────────────────────────
    print("== 1. Server keys / discovery (public) ==")
    record("keys", "/_matrix/federation/v2/server", "GET", "A", "/_matrix/federation/v2/server", public=True)
    record("keys", "/_matrix/key/v2/server", "GET", "A", "/_matrix/key/v2/server", public=True)
    record("keys", "/_matrix/federation/v2/query/{s}/{k}", "GET", "A",
           f"/_matrix/federation/v2/query/{server_b_q}/{q(key_b)}", public=True)
    record("keys", "/_matrix/key/v2/query/{s}/{k}", "GET", "A",
           f"/_matrix/key/v2/query/{server_b_q}/{q(key_b)}", public=True)
    record("keys", "/_matrix/federation/v2/query/{s}", "GET", "A",
           f"/_matrix/federation/v2/query/{server_b_q}", public=True)
    record("keys", "/_matrix/key/v2/query/{s}", "GET", "A",
           f"/_matrix/key/v2/query/{server_b_q}", public=True)
    record("keys", "POST /_matrix/key/v2/query", "POST", "A", "/_matrix/key/v2/query",
           {"server_keys": {INSTANCES["B"]["server_name"]: {}}}, public=True)
    record("discovery", "/_matrix/federation/v1/version", "GET", "A", "/_matrix/federation/v1/version", public=True)
    record("discovery", "/_matrix/federation/v1", "GET", "A", "/_matrix/federation/v1", public=True)
    record("discovery", "GET /publicRooms", "GET", "A", "/_matrix/federation/v1/publicRooms", public=True)
    record("discovery", "/v1/query/destination", "GET", "A", "/_matrix/federation/v1/query/destination", public=True)
    record("openid", "/v1/openid/userinfo", "GET", "A",
           "/_matrix/federation/v1/openid/userinfo?access_token=invalid", public=True, expect=(401,))

    # ── 2. 成员管理（protected）──────────────────────────────────────────────
    # 说明：本组请求以 B 为 origin 发往 A，而 B 在该私有房间中仅为 invited
    # （未 joined）。按 OPT-017 防枚举设计，房间级读取对不可观察 origin 返回
    # 404 "Room not found"（而非 403），故此处对房间级端点声明 404 为预期。
    print("== 2. Membership (protected) ==")
    record("membership", "/members/{room_id}", "GET", "A", f"/_matrix/federation/v1/members/{room_q}", expect=(404,))
    record("membership", "/members/{room_id}/joined", "GET", "A",
           f"/_matrix/federation/v1/members/{room_q}/joined", expect=(404,))
    record("membership", "/user/devices/{user_id} (local)", "GET", "A",
           f"/_matrix/federation/v1/user/devices/{user_a_q}")
    record("membership", "/make_join/{room_id}/{user_id}", "GET", "A",
           f"/_matrix/federation/v1/make_join/{room_q}/{user_b_q}", expect=(404,))
    record("membership", "/make_leave/{room_id}/{user_id}", "GET", "A",
           f"/_matrix/federation/v1/make_leave/{room_q}/{user_b_q}", expect=(404,))
    record("membership", "/knock/{room_id}/{user_id}", "POST", "A",
           f"/_matrix/federation/v1/knock/{room_q}/{user_b_q}", expect=(400, 403, 404))
    record("membership", "POST /thirdparty/invite", "POST", "A",
           "/_matrix/federation/v1/thirdparty/invite",
           {"medium": "email", "address": "nobody@example.com", "room_id": invite["room_id"]},
           expect=(400, 403, 404))
    record("membership", "/v2/invite/{room_id}/{event_id}", "PUT", "A",
           f"/_matrix/federation/v2/invite/{room_q}/{ev_q}",
           {"event": {}, "room_version": "12"}, expect=(400, 403, 404))
    record("membership", "/v1/invite/{room_id}/{event_id}", "PUT", "A",
           f"/_matrix/federation/v1/invite/{room_q}/{ev_q}",
           {"event": {}, "room_version": "12"}, expect=(400, 403, 404))
    record("membership", "/v2/send_join/{room_id}/{event_id}", "PUT", "A",
           f"/_matrix/federation/v2/send_join/{room_q}/{ev_q}", {}, expect=(400, 403, 404))
    record("membership", "/v1/send_join/{room_id}/{event_id}", "PUT", "A",
           f"/_matrix/federation/v1/send_join/{room_q}/{ev_q}", {}, expect=(400, 403, 404))
    record("membership", "/v2/send_leave/{room_id}/{event_id}", "PUT", "A",
           f"/_matrix/federation/v2/send_leave/{room_q}/{ev_q}", {}, expect=(400, 403, 404))
    record("membership", "/v1/send_leave/{room_id}/{event_id}", "PUT", "A",
           f"/_matrix/federation/v1/send_leave/{room_q}/{ev_q}", {}, expect=(400, 403, 404))
    record("membership", "PUT /exchange_third_party_invite/{room_id}", "PUT", "A",
           f"/_matrix/federation/v1/exchange_third_party_invite/{room_q}", {}, expect=(400, 403, 404))

    # ── 3. PDU / 事件 / 状态查询（protected）─────────────────────────────────
    print("== 3. PDU / events / state (protected) ==")
    record("pdu", "/get_missing_events/{room_id}", "POST", "A",
           f"/_matrix/federation/v1/get_missing_events/{room_q}",
           {"earliest_events": [], "latest_events": [invite["invite_event_id"] or "x"]},
           expect=(404,))
    record("pdu", "/room/{room_id}/{event_id}", "GET", "A",
           f"/_matrix/federation/v1/room/{room_q}/{ev_q}", expect=(404,))
    record("pdu", "/timestamp_to_event/{room_id}", "GET", "A",
           f"/_matrix/federation/v1/timestamp_to_event/{room_q}?ts=1790642705720&dir=f",
           expect=(404,))
    record("pdu", "/get_event_auth/{room_id}/{event_id}", "GET", "A",
           f"/_matrix/federation/v1/get_event_auth/{room_q}/{ev_q}", expect=(404,))
    record("pdu", "/state/{room_id}", "GET", "A", f"/_matrix/federation/v1/state/{room_q}", expect=(404,))
    record("pdu", "/event/{event_id}", "GET", "A",
           f"/_matrix/federation/v1/event/{ev_q}", expect=(404,))
    record("pdu", "/state_ids/{room_id}", "GET", "A", f"/_matrix/federation/v1/state_ids/{room_q}",
           expect=(404,))
    record("pdu", "/backfill/{room_id}", "GET", "A",
           f"/_matrix/federation/v1/backfill/{room_q}?v={ev_q}&limit=10", expect=(404,))
    record("pdu", "/send/{txn_id}", "PUT", "A",
           "/_matrix/federation/v1/send/txn-feat-0001",
           {"origin": INSTANCES["B"]["server_name"], "pdus": [], "edus": []})

    # ── 4. profile / directory / hierarchy（protected）──────────────────────
    print("== 4. profile / directory / hierarchy (protected) ==")
    record("profile", "/query/profile", "GET", "A",
           "/_matrix/federation/v1/query/profile?user_id=" + q(USER_A))
    record("profile", "/query/profile/{user_id}", "GET", "A",
           f"/_matrix/federation/v1/query/profile/{user_a_q}")
    record("directory", "/query/directory/room/{room_id}", "GET", "A",
           f"/_matrix/federation/v1/query/directory/room/{room_q}", expect=(404,))
    record("directory", "/query/directory", "GET", "A",
           "/_matrix/federation/v1/query/directory?room_alias=" + q("#fedtest:synapse-a.federation.test"),
           expect=(404,))
    record("hierarchy", "/hierarchy/{room_id}", "GET", "A",
           f"/_matrix/federation/v1/hierarchy/{room_q}", expect=(404,))
    record("publicRooms", "POST /publicRooms", "POST", "A",
           "/_matrix/federation/v1/publicRooms", {"limit": 10})

    # ── 5. E2EE keys（protected）─────────────────────────────────────────────
    print("== 5. E2EE keys (protected) ==")
    record("e2ee", "POST /user/keys/query v1", "POST", "A",
           "/_matrix/federation/v1/user/keys/query", {"device_keys": {USER_B: []}})
    record("e2ee", "POST /user/keys/query v2", "POST", "A",
           "/_matrix/federation/v2/user/keys/query", {"device_keys": {USER_B: []}})
    record("e2ee", "POST /user/keys/claim", "POST", "A",
           "/_matrix/federation/v1/user/keys/claim", {"one_time_keys": {USER_B: {}}})
    record("e2ee", "POST /user/keys/upload", "POST", "A",
           "/_matrix/federation/v1/user/keys/upload", {"device_keys": {}, "one_time_keys": {}},
           expect=(400, 404, 405))

    # ── 6. media（protected）────────────────────────────────────────────────
    print("== 6. media (protected) ==")
    record("media", "/media/download/{s}/{id}", "GET", "A",
           f"/_matrix/federation/v1/media/download/{server_a_q}/{q('nonexistentmediaid')}",
           expect=(404,))
    record("media", "/media/thumbnail/{s}/{id}", "GET", "A",
           f"/_matrix/federation/v1/media/thumbnail/{server_a_q}/{q('nonexistentmediaid')}",
           expect=(404,))

    # ── 7. 非标准扩展 `_synapse/federation/*`（经应用端口）──────────────────
    print("== 7. _synapse/federation/* extensions (via app port) ==")
    record("synapse-ext", "v2/key/clone", "POST", "A", "/_synapse/federation/v2/key/clone",
           {}, via="app", expect=(404,))
    record("synapse-ext", "v1/keys/claim (stub)", "POST", "A", "/_synapse/federation/v1/keys/claim",
           {}, via="app", expect=(400,))
    record("synapse-ext", "v1/keys/query (stub)", "POST", "A", "/_synapse/federation/v1/keys/query",
           {}, via="app", expect=(400,))
    record("synapse-ext", "v1/keys/upload (stub)", "POST", "A", "/_synapse/federation/v1/keys/upload",
           {}, via="app", expect=(400,))
    record("synapse-ext", "v1/room_auth/{room_id}", "GET", "A",
           f"/_synapse/federation/v1/room_auth/{room_q}", via="app", expect=(404,))
    record("synapse-ext", "v1/query/auth (stub)", "GET", "A", "/_synapse/federation/v1/query/auth",
           via="app", expect=(400, 404))
    record("synapse-ext", "v1/get_joining_rules/{room_id}", "GET", "A",
           f"/_synapse/federation/v1/get_joining_rules/{room_q}", via="app", expect=(404,))
    # 反证：经联邦端口应被 nginx 拦截为 404（设计如此，无出站调用方）
    record("synapse-ext", "get_joining_rules via 8448 (nginx)", "GET", "A",
           f"/_synapse/federation/v1/get_joining_rules/{room_q}", via="fed", expect=(404,))

    # ── 输出汇总 ─────────────────────────────────────────────────────────────
    print()
    print("=" * 118)
    print(f"{'category':<12} {'endpoint':<44} {'method':<6} {'verdict':<10} {'code':<5} body")
    print("=" * 118)
    for cat, endpoint, method, v, code, preview in results:
        print(f"{cat:<12} {endpoint:<44} {method:<6} {v:<10} {code:<5} {preview}")

    fails = [r for r in results
             if r[3] in ("AUTH_FAIL", "SERVER_ERROR", "TRANSPORT", "METHOD_NA")]
    unexpected = [r for r in results if r[3] not in ("PASS", "EXPECTED")]
    print()
    print(f"Total: {len(results)}  PASS/EXPECTED: {len(results) - len(unexpected)}  "
          f"hard-fail(AUTH/SERVER/TRANSPORT): {len(fails)}")
    if unexpected:
        print("-- 非 PASS 且未声明预期 --")
        for r in unexpected:
            print(f"  [{r[3]}] {r[0]} {r[1]} -> {r[4]}: {r[5]}")
    if fails:
        print("-- 硬失败 --")
        for r in fails:
            print(f"  [FAIL] {r[0]} {r[1]} -> {r[3]} ({r[4]}): {r[5]}")
    return 0 if not fails else 1


if __name__ == "__main__":
    raise SystemExit(main())
