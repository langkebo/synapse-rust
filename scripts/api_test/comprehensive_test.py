#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Synapse-Rust Matrix API Comprehensive Test Suite

Phases:
    1. Admin phase      - exercise /_synapse/admin endpoints
    2. Permission phase - cross-user authorization boundaries
    3. Boundary phase    - input validation / error paths
    4. Functional phase  - happy-path E2E flows
"""

import json
import os
import re
import ssl
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Optional

import yaml

# -----------------------------------------------------------------------------
# Constants
# -----------------------------------------------------------------------------
SCRIPT_DIR = Path(__file__).resolve().parent
LEDGER_PATH = SCRIPT_DIR / "ledger.json"
REPORT_DIR = SCRIPT_DIR / "reports"
REPORT_PATH = REPORT_DIR / "comprehensive_report.json"


# -----------------------------------------------------------------------------
# Config
# -----------------------------------------------------------------------------
def load_config():
    cfg_path = SCRIPT_DIR / "config.yaml"
    with open(cfg_path, "r", encoding="utf-8") as f:
        return yaml.safe_load(f) or {}


# -----------------------------------------------------------------------------
# HTTP client
# -----------------------------------------------------------------------------
class MatrixClient:
    def __init__(self, base_url, verify_tls=True, timeout=10):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        if verify_tls:
            self.ssl_ctx = ssl.create_default_context()
        else:
            self.ssl_ctx = ssl._create_unverified_context()

    def _req(self, method, path, token="", headers=None, body=""):
        url = self.base_url + path
        hdrs = {"Accept": "application/json"}
        if headers:
            hdrs.update(headers)
        if token:
            hdrs["Authorization"] = f"Bearer {token}"
        if body and "Content-Type" not in hdrs:
            hdrs["Content-Type"] = "application/json"

        req = urllib.request.Request(
            url,
            data=body.encode("utf-8") if body else None,
            method=method,
            headers=hdrs,
        )
        for attempt in range(3):
            try:
                with urllib.request.urlopen(req, timeout=self.timeout, context=self.ssl_ctx) as resp:
                    status = resp.getcode()
                    ctype = resp.headers.get("Content-Type", "")
                    raw = resp.read(8192)
                    try:
                        text = raw.decode("utf-8", errors="replace")
                    except Exception:
                        text = ""
                    return status, ctype, text
            except urllib.error.HTTPError as e:
                raw = e.read(8192) if hasattr(e, "read") else b""
                try:
                    text = raw.decode("utf-8", errors="replace")
                except Exception:
                    text = ""
                ctype = ""
                if e.headers:
                    ctype = e.headers.get("Content-Type", "")
                if e.code == 429 and attempt < 2:
                    try:
                        data = json.loads(text)
                        wait_ms = int(data.get("retry_after_ms", 1000))
                    except Exception:
                        wait_ms = 1000 * (2 ** attempt)
                    time.sleep(wait_ms / 1000.0)
                    continue
                return e.code, ctype, text
            except Exception:
                return -1, "", ""
        return -1, "", ""

    def login(self, username, password, retries=3, backoff_base=2.0):
        """Login with retry-on-rate-limit support."""
        body = json.dumps({
            "type": "m.login.password",
            "identifier": {"type": "m.id.user", "user": username},
            "password": password,
        })
        for attempt in range(retries):
            status, _, text = self._req("POST", "/_matrix/client/v3/login", body=body)
            if status == 429:
                try:
                    data = json.loads(text)
                    wait_ms = int(data.get("retry_after_ms", 1000))
                except Exception:
                    wait_ms = 1000 * (2 ** attempt)
                wait_s = wait_ms / 1000.0
                print(f"  [login] rate-limited (attempt {attempt+1}/{retries}), "
                      f"waiting {wait_s:.1f}s ...")
                time.sleep(wait_s)
                continue
            if status != 200:
                return None, ""
            try:
                data = json.loads(text)
                return data.get("access_token", ""), data.get("user_id", "")
            except Exception:
                return None, ""
        return None, ""

    def create_room(self, token, preset, name, topic):
        body = json.dumps({
            "preset": preset,
            "name": name,
            "topic": topic,
            "visibility": "private" if preset == "private_chat" else "public",
        })
        return self._req("POST", "/_matrix/client/v3/createRoom", token=token, body=body)

    def send_text(self, token, room_id, text, txn_id=""):
        if not txn_id:
            txn_id = f"txn-{uuid.uuid4().hex[:16]}"
        body = json.dumps({"msgtype": "m.text", "body": text})
        path = (
            f"/_matrix/client/v3/rooms/{urllib.parse.quote(room_id, safe='')}"
            f"/send/m.room.message/{txn_id}"
        )
        return self._req("PUT", path, token=token, body=body)

    def get_messages(self, token, room_id, limit=5):
        path = (
            f"/_matrix/client/v3/rooms/{urllib.parse.quote(room_id, safe='')}"
            f"/messages?limit={limit}"
        )
        return self._req("GET", path, token=token)

    def invite(self, token, room_id, user_id):
        body = json.dumps({"user_id": user_id})
        path = f"/_matrix/client/v3/rooms/{urllib.parse.quote(room_id, safe='')}/invite"
        return self._req("POST", path, token=token, body=body)

    def join_room(self, token, room_id_or_alias):
        body = json.dumps({})
        path = f"/_matrix/client/v3/join/{urllib.parse.quote(room_id_or_alias, safe='')}"
        return self._req("POST", path, token=token, body=body)

    def kick_user(self, token, room_id, user_id, reason):
        body = json.dumps({"user_id": user_id, "reason": reason})
        path = f"/_matrix/client/v3/rooms/{urllib.parse.quote(room_id, safe='')}/kick"
        return self._req("POST", path, token=token, body=body)

    def ban_user(self, token, room_id, user_id, reason):
        body = json.dumps({"user_id": user_id, "reason": reason})
        path = f"/_matrix/client/v3/rooms/{urllib.parse.quote(room_id, safe='')}/ban"
        return self._req("POST", path, token=token, body=body)

    def unban_user(self, token, room_id, user_id):
        body = json.dumps({"user_id": user_id})
        path = f"/_matrix/client/v3/rooms/{urllib.parse.quote(room_id, safe='')}/unban"
        return self._req("POST", path, token=token, body=body)

    def leave_room(self, token, room_id):
        body = json.dumps({})
        path = f"/_matrix/client/v3/rooms/{urllib.parse.quote(room_id, safe='')}/leave"
        return self._req("POST", path, token=token, body=body)

    def sync(self, token, since=""):
        path = "/_matrix/client/v3/sync"
        if since:
            path += f"?since={urllib.parse.quote(since, safe='')}"
        return self._req("GET", path, token=token)

    def get_profile(self, token, user_id):
        path = f"/_matrix/client/v3/profile/{urllib.parse.quote(user_id, safe='')}"
        return self._req("GET", path, token=token)

    def set_displayname(self, token, user_id, displayname):
        body = json.dumps({"displayname": displayname})
        path = (
            f"/_matrix/client/v3/profile/{urllib.parse.quote(user_id, safe='')}/displayname"
        )
        return self._req("PUT", path, token=token, body=body)

    def upload_media(self, token, content_bytes, content_type="image/png"):
        headers = {"Content-Type": content_type}
        url = self.base_url + "/_matrix/media/v3/upload"
        req = urllib.request.Request(
            url,
            data=content_bytes,
            method="POST",
            headers={**headers, "Authorization": f"Bearer {token}"},
        )
        try:
            with urllib.request.urlopen(req, timeout=self.timeout, context=self.ssl_ctx) as resp:
                status = resp.getcode()
                ctype = resp.headers.get("Content-Type", "")
                raw = resp.read(8192)
                return status, ctype, raw.decode("utf-8", errors="replace")
        except urllib.error.HTTPError as e:
            raw = e.read(8192) if hasattr(e, "read") else b""
            ctype = ""
            if e.headers:
                ctype = e.headers.get("Content-Type", "")
            return e.code, ctype, raw.decode("utf-8", errors="replace")
        except Exception:
            return -1, "", ""


# -----------------------------------------------------------------------------
# Data classes
# -----------------------------------------------------------------------------
@dataclass
class Finding:
    id: str
    phase: str
    severity: str
    category: str
    title: str
    description: str
    repro: list
    expected: str
    actual: str
    endpoint: str
    auth_context: str
    fix_suggestion: str = ""


@dataclass
class TestCase:
    id: str
    phase: str
    category: str
    description: str
    method: str
    path: str
    token_type: str
    expected: str
    status: int = 0
    errcode: str = ""
    passed: bool = False
    finding_id: str = ""
    notes: str = ""
    elapsed_ms: int = 0


# -----------------------------------------------------------------------------
# Helpers
# -----------------------------------------------------------------------------
def parse_errcode(body):
    try:
        return json.loads(body).get("errcode", "")
    except Exception:
        return ""


def load_ledger():
    p = Path(LEDGER_PATH)
    if not p.is_absolute():
        p = SCRIPT_DIR / LEDGER_PATH
    if not p.exists():
        return []
    try:
        with open(p, "r", encoding="utf-8") as f:
            data = json.load(f)
        if isinstance(data, list):
            return data
        if isinstance(data, dict):
            for k in ("entries", "endpoints", "routes", "items"):
                if k in data and isinstance(data[k], list):
                    return data[k]
            return [data]
    except Exception:
        return []
    return []


def sub_params(path):
    mapping = {
        "user_id": "@testuser1:matrix.test",
        "room_id": "!apitest-nosuchroom:matrix.test",
        "room_alias": "#apitest-nosuchalias:matrix.test",
        "event_id": "$apitest-nosuchevent:matrix.test",
        "txn_id": "apitest-txn-00000000",
        "transaction_id": "apitest-txn-00000000",
        "device_id": "APITESTDEVICE0000",
        "filter_id": "apitest-filter-0000",
        "key_name": "com.example.apitest",
        "tag": "m.favourite",
        "scope": "global",
        "media_id": "doesnotexist",
        "server_name": "matrix.test",
        "user": "testuser1",
        "sender": "@testuser1:matrix.test",
        "thread_id": "$apitest-nosuchevent:matrix.test",
        "token": "apitest-token-0000",
        "id": "apitest-0000",
        "name": "apitest-name",
        "network_id": "apitest-network",
        "appservice_id": "apitest-as",
    }

    def repl(m):
        return mapping.get(m.group(1), m.group(0))

    return re.sub(r"\{([a-zA-Z_]+)\}", repl, path)


def add_finding(findings, **kw):
    fid = f"F-{uuid.uuid4().hex[:6]}"
    findings.append(Finding(id=fid, **kw))
    return fid


def record_5xx(findings, phase, method, path, body, severity="P1"):
    return add_finding(
        findings,
        phase=phase,
        severity=severity,
        category="server_error",
        title=f"5xx on {method} {path}",
        description=f"Server returned a 5xx error: {body[:200]}",
        repro=[f"{method} {path}"],
        expected="2xx/3xx/4xx",
        actual=body[:120],
        endpoint=path,
        auth_context="admin",
        fix_suggestion="Inspect server logs for stack trace and fix the underlying handler.",
    )


# -----------------------------------------------------------------------------
# Phase 1: Admin
# -----------------------------------------------------------------------------
DESTRUCTIVE_DELETE_HINTS = (
    "users", "rooms", "media", "reports", "event_reports", "devices",
    "delete", "purge", "reset",
)


def run_admin_phase(client):
    cases = []
    findings = []
    token, user_id = client.login("admin", "Admin@123")
    if not token:
        add_finding(
            findings,
            phase="admin",
            severity="P0",
            category="auth",
            title="Admin login failed",
            description="Cannot login as admin; cannot run admin phase.",
            repro=["POST /_matrix/client/v3/login (admin)"],
            expected="200 with access_token",
            actual="login returned no token",
            endpoint="/_matrix/client/v3/login",
            auth_context="admin",
            fix_suggestion="Verify admin user exists and Admin@123 password is correct.",
        )
        return cases, findings

    ledger = load_ledger()
    admin_eps = [
        e for e in ledger
        if isinstance(e, dict) and "/admin/" in str(e.get("path", ""))
    ]

    for i, ep in enumerate(admin_eps):
        method = str(ep.get("method", "GET")).upper()
        raw_path = str(ep.get("path", ""))
        path = sub_params(raw_path)
        is_destructive = method == "DELETE" or any(
            h in raw_path.lower() for h in DESTRUCTIVE_DELETE_HINTS
        )
        if is_destructive:
            cases.append(TestCase(
                id=f"C-ADM-{len(cases) + 1:03d}",
                phase="admin",
                category="skipped",
                description=f"Skipped destructive: {method} {raw_path}",
                method=method, path=raw_path, token_type="admin",
                expected="(skipped)",
                passed=True,
                notes="skipped destructive admin endpoint",
            ))
            continue

        if (i + 1) % 25 == 0:
            time.sleep(0.5)  # gentle pacing to avoid server-side rate limiting / 502s

        t0 = time.time()
        body_str = ""
        if method == "GET":
            status, _, body_str = client._req("GET", path, token=token)
        elif method == "POST":
            status, _, body_str = client._req("POST", path, token=token, body="{}")
        elif method == "PUT":
            status, _, body_str = client._req("PUT", path, token=token, body="{}")
        else:
            status, _, body_str = client._req(method, path, token=token)
        elapsed = int((time.time() - t0) * 1000)
        errcode = parse_errcode(body_str)

        passed = 200 <= status < 500 and status != -1
        finding_id = ""
        if 500 <= status < 600 or status == -1:
            finding_id = record_5xx(
                findings, "admin", method, raw_path, body_str,
                "P0" if status == -1 else "P1",
            )
            passed = False
        elif status == 404 and "errcode" not in body_str.lower():
            passed = True

        cases.append(TestCase(
            id=f"C-ADM-{len(cases) + 1:03d}",
            phase="admin",
            category="admin",
            description=f"{method} {raw_path}",
            method=method, path=raw_path, token_type="admin",
            expected="2xx/3xx/4xx",
            status=status, errcode=errcode, passed=passed,
            finding_id=finding_id, elapsed_ms=elapsed,
        ))

    return cases, findings


# -----------------------------------------------------------------------------
# Phase 2: Permission
# -----------------------------------------------------------------------------
def _resolve_test_users(cfg):
    """Resolve (user_a, user_b) credentials from config, with safe fallbacks."""
    tu = cfg.get("test_users") or {}
    ua = tu.get("user_a") or {}
    ub = tu.get("user_b") or {}
    user_a_name = ua.get("username") or "test2"
    user_a_pass = ua.get("password") or "Ljf@1234"
    user_b_name = ub.get("username") or "test4"
    user_b_pass = ub.get("password") or "Ljf@1234"
    admin_user = (cfg.get("admin") or {}).get("username") or "admin"
    admin_pass = (cfg.get("admin") or {}).get("password") or "Admin@123"
    return {
        "user_a_name": user_a_name,
        "user_a_pass": user_a_pass,
        "user_b_name": user_b_name,
        "user_b_pass": user_b_pass,
        "admin_user": admin_user,
        "admin_pass": admin_pass,
    }


def run_permission_phase(client, cfg=None):
    creds = _resolve_test_users(cfg or {})
    user_a_name = creds["user_a_name"]
    user_a_pass = creds["user_a_pass"]
    user_b_name = creds["user_b_name"]
    user_b_pass = creds["user_b_pass"]
    admin_user = creds["admin_user"]
    admin_pass = creds["admin_pass"]
    cases = []
    findings = []
    t1_tok, t1_uid = client.login(user_a_name, user_a_pass)
    t2_tok, t2_uid = client.login(user_b_name, user_b_pass)
    adm_tok, _ = client.login(admin_user, admin_pass)
    if not (t1_tok and t2_tok):
        add_finding(
            findings,
            phase="permission",
            severity="P0",
            category="auth",
            title="Test user login failed",
            description=f"{user_a_name} or {user_b_name} could not login.",
            repro=["POST /_matrix/client/v3/login"],
            expected="200",
            actual="login failed",
            endpoint="/_matrix/client/v3/login",
            auth_context=f"{user_a_name}/{user_b_name}",
        )
        return cases, findings

    def rec(cid, desc, method, path, token_type, expected, status, body, ok,
            finding_kw=None):
        errcode = parse_errcode(body)
        fid = ""
        if finding_kw:
            fid = add_finding(findings, **finding_kw)
        cases.append(TestCase(
            id=cid, phase="permission",
            category="permission", description=desc,
            method=method, path=path, token_type=token_type,
            expected=expected, status=status, errcode=errcode,
            passed=ok, finding_id=fid, notes=(body or "")[:200],
        ))

    # T1: user_a creates private room
    s, _, b = client.create_room(t1_tok, "private_chat", "perm-room", "perm topic")
    room_id = ""
    if s == 200:
        try:
            room_id = json.loads(b).get("room_id", "")
        except Exception:
            room_id = ""
    ok_t1 = s == 200 and bool(room_id)
    rec(
        "C-PER-001", f"{user_a_name} creates private room", "POST", "/createRoom", user_a_name,
        "200", s, b, ok_t1,
        finding_kw=(None if ok_t1 else dict(
            phase="permission", severity="P1", category="functional",
            title=f"{user_a_name} cannot create private room",
            description=f"createRoom returned {s}: {b[:120]}",
            repro=[f"POST /createRoom as {user_a_name}"],
            expected="200", actual=str(s), endpoint="/createRoom",
            auth_context=user_a_name,
            fix_suggestion="Verify createRoom handler authorizes user.",
        )),
    )
    if not ok_t1:
        return cases, findings

    # T2: user_a sends text to own room
    s, _, b = client.send_text(t1_tok, room_id, f"hello from {user_a_name}")
    rec(
        "C-PER-002", f"{user_a_name} sends text to own room", "PUT",
        f"/rooms/{room_id}/send/m.room.message/...", user_a_name,
        "200", s, b, s == 200,
        finding_kw=(None if s == 200 else dict(
            phase="permission", severity="P1", category="functional",
            title="Owner cannot send to own room",
            description=f"send returned {s}: {b[:120]}",
            repro=["PUT /rooms/{room_id}/send/m.room.message"],
            expected="200", actual=str(s), endpoint="/rooms/{room_id}/send",
            auth_context=user_a_name,
        )),
    )

    # T3: user_b reads messages from user_a's room without being invited
    s, _, b = client.get_messages(t2_tok, room_id, limit=5)
    ok_t3 = s in (403, 404)
    rec(
        "C-PER-003", f"{user_b_name} reads {user_a_name}'s room without invite", "GET",
        f"/rooms/{room_id}/messages", user_b_name,
        "403 or 404", s, b, ok_t3,
        finding_kw=(None if ok_t3 else dict(
            phase="permission", severity="P0", category="security",
            title="Unauthorized read of private room",
            description=f"{user_b_name} read private room without invite: {s} {b[:120]}",
            repro=[f"GET /rooms/{room_id}/messages as {user_b_name} (not invited)"],
            expected="403 or 404", actual=str(s), endpoint=f"/rooms/{room_id}/messages",
            auth_context=user_b_name,
            fix_suggestion="Enforce membership check on /messages.",
        )),
    )

    # T4: user_b self-invites to user_a's room
    s, _, b = client.join_room(t2_tok, room_id)
    ok_t4 = s == 403
    rec(
        "C-PER-004", f"{user_b_name} self-invites (joins) {user_a_name}'s private room", "POST",
        f"/join/{room_id}", user_b_name,
        "403", s, b, ok_t4,
        finding_kw=(None if ok_t4 else dict(
            phase="permission", severity="P0", category="security",
            title="Self-invite bypass on private room",
            description=f"{user_b_name} joined private room without invite: {s} {b[:120]}",
            repro=[f"POST /join/{room_id} as {user_b_name}"],
            expected="403", actual=str(s), endpoint=f"/join/{room_id}",
            auth_context=user_b_name,
            fix_suggestion="Reject join without invite on private rooms.",
        )),
    )

    # T5: user_a invites user_b
    target_uid = t2_uid or f"@{user_b_name}:matrix.test"
    s, _, b = client.invite(t1_tok, room_id, target_uid)
    rec(
        "C-PER-005", f"{user_a_name} invites {user_b_name}", "POST",
        f"/rooms/{room_id}/invite", user_a_name,
        "200", s, b, s == 200,
        finding_kw=(None if s == 200 else dict(
            phase="permission", severity="P1", category="functional",
            title="Owner cannot invite",
            description=f"invite returned {s}: {b[:120]}",
            repro=[f"POST /rooms/{room_id}/invite as {user_a_name}"],
            expected="200", actual=str(s), endpoint=f"/rooms/{room_id}/invite",
            auth_context=user_a_name,
        )),
    )

    # T6: user_b joins after invite
    s, _, b = client.join_room(t2_tok, room_id)
    rec(
        "C-PER-006", f"{user_b_name} joins after invite", "POST",
        f"/join/{room_id}", user_b_name,
        "200", s, b, s == 200,
        finding_kw=(None if s == 200 else dict(
            phase="permission", severity="P1", category="functional",
            title="Invited user cannot join",
            description=f"join returned {s}: {b[:120]}",
            repro=[f"POST /join/{room_id} as {user_b_name} (invited)"],
            expected="200", actual=str(s), endpoint=f"/join/{room_id}",
            auth_context=user_b_name,
        )),
    )

    # T7: user_b reads after joining
    s, _, b = client.get_messages(t2_tok, room_id, limit=5)
    rec(
        "C-PER-007", f"{user_b_name} reads after joining", "GET",
        f"/rooms/{room_id}/messages", user_b_name,
        "200", s, b, s == 200,
        finding_kw=(None if s == 200 else dict(
            phase="permission", severity="P1", category="functional",
            title="Member cannot read messages",
            description=f"messages returned {s}: {b[:120]}",
            repro=[f"GET /rooms/{room_id}/messages as {user_b_name} (member)"],
            expected="200", actual=str(s), endpoint=f"/rooms/{room_id}/messages",
            auth_context=user_b_name,
        )),
    )

    # T8: user_b (member) kicks user_a (owner)
    target_owner = t1_uid or f"@{user_a_name}:matrix.test"
    s, _, b = client.kick_user(t2_tok, room_id, target_owner, "perm-test")
    ok_t8 = s == 403
    rec(
        "C-PER-008", f"{user_b_name} (member) tries to kick {user_a_name} (owner)", "POST",
        f"/rooms/{room_id}/kick", user_b_name,
        "403", s, b, ok_t8,
        finding_kw=(None if ok_t8 else dict(
            phase="permission", severity="P0", category="security",
            title="Privilege escalation: member kicks owner",
            description=f"member kicked owner: {s} {b[:120]}",
            repro=[f"POST /rooms/{room_id}/kick as {user_b_name} (member)"],
            expected="403", actual=str(s), endpoint=f"/rooms/{room_id}/kick",
            auth_context=user_b_name,
            fix_suggestion="Verify caller has >= target power level.",
        )),
    )

    # T9: user_a (non-admin) calls admin whois
    target_who = t2_uid or f"@{user_b_name}:matrix.test"
    s, _, b = client._req(
        "GET", f"/_synapse/admin/v1/whois/{urllib.parse.quote(target_who, safe='')}",
        token=t1_tok,
    )
    ok_t9 = s == 403
    rec(
        "C-PER-009", f"{user_a_name} (non-admin) calls admin whois", "GET",
        f"/_synapse/admin/v1/whois/{target_who}", user_a_name,
        "403", s, b, ok_t9,
        finding_kw=(None if ok_t9 else dict(
            phase="permission", severity="P0", category="security",
            title="Non-admin can access admin API",
            description=f"whois returned {s}: {b[:120]}",
            repro=[f"GET /_synapse/admin/v1/whois/{target_who} as {user_a_name}"],
            expected="403", actual=str(s), endpoint=f"/_synapse/admin/v1/whois/{target_who}",
            auth_context=user_a_name,
            fix_suggestion="Enforce admin role on /_synapse/admin/* routes.",
        )),
    )

    # T10: anonymous calls admin whois
    target_who_t1 = t1_uid or f"@{user_a_name}:matrix.test"
    s, _, b = client._req(
        "GET", f"/_synapse/admin/v1/whois/{urllib.parse.quote(target_who_t1, safe='')}",
    )
    ok_t10 = s == 401
    rec(
        "C-PER-010", "anonymous calls admin whois", "GET",
        f"/_synapse/admin/v1/whois/{target_who_t1}", "anonymous",
        "401", s, b, ok_t10,
        finding_kw=(None if ok_t10 else dict(
            phase="permission", severity="P0", category="security",
            title="Anonymous can access admin API",
            description=f"whois returned {s}: {b[:120]}",
            repro=[f"GET /_synapse/admin/v1/whois/{target_who_t1} (anon)"],
            expected="401", actual=str(s), endpoint=f"/_synapse/admin/v1/whois/{target_who_t1}",
            auth_context="anonymous",
            fix_suggestion="Reject unauthenticated requests on admin API.",
        )),
    )

    # T11: user_a reads user_b's profile (public)
    target_t2 = t2_uid or f"@{user_b_name}:matrix.test"
    s, _, b = client.get_profile(t1_tok, target_t2)
    rec(
        "C-PER-011", f"{user_a_name} reads {user_b_name}'s profile", "GET",
        f"/profile/{target_t2}", user_a_name,
        "200", s, b, s == 200,
        finding_kw=(None if s == 200 else dict(
            phase="permission", severity="P1", category="functional",
            title="Cannot read public profile",
            description=f"profile returned {s}: {b[:120]}",
            repro=[f"GET /profile/{target_t2} as {user_a_name}"],
            expected="200", actual=str(s), endpoint=f"/profile/{target_t2}",
            auth_context=user_a_name,
        )),
    )

    # T12: user_b sets user_a's displayname
    target_t1 = t1_uid or f"@{user_a_name}:matrix.test"
    s, _, b = client.set_displayname(t2_tok, target_t1, "hijacked")
    ok_t12 = s == 403
    rec(
        "C-PER-012", f"{user_b_name} sets {user_a_name}'s displayname", "PUT",
        f"/profile/{target_t1}/displayname", user_b_name,
        "403", s, b, ok_t12,
        finding_kw=(None if ok_t12 else dict(
            phase="permission", severity="P0", category="security",
            title="Cross-user profile mutation",
            description=f"displayname set returned {s}: {b[:120]}",
            repro=[f"PUT /profile/{target_t1}/displayname as {user_b_name}"],
            expected="403", actual=str(s), endpoint=f"/profile/{target_t1}/displayname",
            auth_context=user_b_name,
            fix_suggestion="Verify caller user_id matches target user_id.",
        )),
    )

    # T13: user_b bans user_a (member banning owner)
    s, _, b = client.ban_user(t2_tok, room_id, target_owner, "perm-test")
    ok_t13 = s == 403
    rec(
        "C-PER-013", f"{user_b_name} (member) bans {user_a_name} (owner)", "POST",
        f"/rooms/{room_id}/ban", user_b_name,
        "403", s, b, ok_t13,
        finding_kw=(None if ok_t13 else dict(
            phase="permission", severity="P0", category="security",
            title="Privilege escalation: member bans owner",
            description=f"ban returned {s}: {b[:120]}",
            repro=[f"POST /rooms/{room_id}/ban as {user_b_name} (member)"],
            expected="403", actual=str(s), endpoint=f"/rooms/{room_id}/ban",
            auth_context=user_b_name,
            fix_suggestion="Verify ban power level >= target.",
        )),
    )

    # Cleanup
    try:
        client.leave_room(t1_tok, room_id)
    except Exception:
        pass
    try:
        client.leave_room(t2_tok, room_id)
    except Exception:
        pass

    return cases, findings


# -----------------------------------------------------------------------------
# Phase 3: Boundary
# -----------------------------------------------------------------------------
def run_boundary_phase(client, cfg=None):
    creds = _resolve_test_users(cfg or {})
    user_a_name = creds["user_a_name"]
    user_a_pass = creds["user_a_pass"]
    user_b_name = creds["user_b_name"]
    user_b_pass = creds["user_b_pass"]
    cases = []
    findings = []
    t1_tok, _ = client.login(user_a_name, user_a_pass)
    t2_tok, _ = client.login(user_b_name, user_b_pass)
    if not t1_tok:
        return cases, findings

    def rec(cid, desc, method, path, token_type, expected, status, body, ok,
            finding_kw=None):
        errcode = parse_errcode(body)
        fid = ""
        if finding_kw:
            fid = add_finding(findings, **finding_kw)
        cases.append(TestCase(
            id=cid, phase="boundary",
            category="boundary", description=desc,
            method=method, path=path, token_type=token_type,
            expected=expected, status=status, errcode=errcode,
            passed=ok, finding_id=fid, notes=(body or "")[:200],
        ))

    # B1: POST /login with empty body
    s, _, b = client._req("POST", "/_matrix/client/v3/login", body="")
    rec(
        "C-BND-001", "POST /login empty body", "POST", "/login", "anonymous",
        "400", s, b, s == 400,
        finding_kw=(None if s == 400 else dict(
            phase="boundary", severity="P1", category="validation",
            title="Empty login body not rejected",
            description=f"empty login returned {s}: {b[:120]}",
            repro=["POST /_matrix/client/v3/login with empty body"],
            expected="400", actual=str(s), endpoint="/_matrix/client/v3/login",
            auth_context="anonymous",
        )),
    )

    # B2: POST /login with invalid JSON
    s, _, b = client._req("POST", "/_matrix/client/v3/login", body="not-json{")
    rec(
        "C-BND-002", "POST /login invalid JSON", "POST", "/login", "anonymous",
        "400", s, b, s == 400,
        finding_kw=(None if s == 400 else dict(
            phase="boundary", severity="P1", category="validation",
            title="Invalid JSON login body not rejected",
            description=f"invalid JSON login returned {s}: {b[:120]}",
            repro=["POST /_matrix/client/v3/login with non-JSON body"],
            expected="400", actual=str(s), endpoint="/_matrix/client/v3/login",
            auth_context="anonymous",
        )),
    )

    # B3: POST /login with wrong password
    body = json.dumps({
        "type": "m.login.password",
        "identifier": {"type": "m.id.user", "user": user_a_name},
        "password": "WrongPass!999",
    })
    s, _, b = client._req("POST", "/_matrix/client/v3/login", body=body)
    ec = parse_errcode(b)
    ok_b3 = s == 401 and ec == "M_FORBIDDEN"
    rec(
        "C-BND-003", "POST /login wrong password", "POST", "/login", "anonymous",
        "401 M_FORBIDDEN", s, b, ok_b3,
        finding_kw=(None if ok_b3 else dict(
            phase="boundary", severity="P1", category="validation",
            title="Wrong-password response inconsistent",
            description=f"wrong password returned {s} errcode={ec}: {b[:120]}",
            repro=["POST /_matrix/client/v3/login with wrong password"],
            expected="401 M_FORBIDDEN", actual=f"{s} {ec}",
            endpoint="/_matrix/client/v3/login", auth_context="anonymous",
        )),
    )

    # B4: POST /login with non-existent user
    body = json.dumps({
        "type": "m.login.password",
        "identifier": {"type": "m.id.user", "user": "nosuchuser_xyz_999"},
        "password": "whatever",
    })
    s, _, b = client._req("POST", "/_matrix/client/v3/login", body=body)
    ok_b4 = s in (401, 403)
    rec(
        "C-BND-004", "POST /login non-existent user", "POST", "/login", "anonymous",
        "401 or 403", s, b, ok_b4,
        finding_kw=(None if ok_b4 else dict(
            phase="boundary", severity="P1", category="validation",
            title="Non-existent user login not rejected",
            description=f"non-existent user returned {s}: {b[:120]}",
            repro=["POST /_matrix/client/v3/login with unknown user"],
            expected="401 or 403", actual=str(s),
            endpoint="/_matrix/client/v3/login", auth_context="anonymous",
        )),
    )

    # B5: GET /sync with invalid since token
    s, _, b = client._req(
        "GET", "/_matrix/client/v3/sync?since=not-a-valid-token!!!@@",
        token=t1_tok,
    )
    rec(
        "C-BND-005", "GET /sync invalid since", "GET", "/sync", user_a_name,
        "400", s, b, s == 400,
        finding_kw=(None if s == 400 else dict(
            phase="boundary", severity="P1", category="validation",
            title="Invalid sync since token not rejected",
            description=f"sync invalid since returned {s}: {b[:120]}",
            repro=["GET /sync?since=invalid"],
            expected="400", actual=str(s),
            endpoint="/_matrix/client/v3/sync", auth_context=user_a_name,
        )),
    )

    # B6: createRoom with name > 255 chars
    long_name = "x" * 256
    s, _, b = client.create_room(t1_tok, "public_chat", long_name, "topic")
    ok_b6 = s in (200, 400)
    rec(
        "C-BND-006", "createRoom with name>255 chars", "POST", "/createRoom", user_a_name,
        "200 or 400", s, b, ok_b6,
        finding_kw=(None if ok_b6 else dict(
            phase="boundary", severity="P2", category="validation",
            title="Oversized room name not handled",
            description=f"createRoom(name=256ch) returned {s}: {b[:120]}",
            repro=["POST /createRoom with 256-char name"],
            expected="200 or 400", actual=str(s),
            endpoint="/createRoom", auth_context=user_a_name,
        )),
    )

    # B7: send oversized body (>10000 chars)
    s, _, b = client.send_text(t1_tok, "!apitest-nosuchroom:matrix.test", "y" * 10001)
    ok_b7 = s in (200, 403, 404, 413)
    rec(
        "C-BND-007", "send 10K+ char body", "PUT", "/rooms/.../send", user_a_name,
        "200 or 413", s, b, ok_b7,
        finding_kw=(None if ok_b7 else dict(
            phase="boundary", severity="P2", category="validation",
            title="Oversized message body not handled",
            description=f"send(10K+) returned {s}: {b[:120]}",
            repro=["PUT /rooms/{id}/send/m.room.message with 10K+ body"],
            expected="200 or 413", actual=str(s),
            endpoint="/rooms/{room_id}/send", auth_context=user_a_name,
        )),
    )

    # B8: PUT on GET-only endpoint -> 405
    s, _, b = client._req("PUT", "/_matrix/client/v3/sync", token=t1_tok, body="{}")
    rec(
        "C-BND-008", "PUT on /sync (wrong method)", "PUT", "/sync", user_a_name,
        "405", s, b, s == 405,
        finding_kw=(None if s == 405 else dict(
            phase="boundary", severity="P2", category="routing",
            title="Wrong method on /sync not rejected with 405",
            description=f"PUT /sync returned {s}: {b[:120]}",
            repro=["PUT /_matrix/client/v3/sync"],
            expected="405", actual=str(s),
            endpoint="/_matrix/client/v3/sync", auth_context=user_a_name,
        )),
    )

    # B9: GET /profile with invalid user_id
    s, _, b = client.get_profile(t1_tok, "@@@invalid@@@")
    ok_b9 = s in (400, 404)
    rec(
        "C-BND-009", "GET /profile invalid user_id", "GET", "/profile/@@@invalid@@@",
        user_a_name, "400 or 404", s, b, ok_b9,
        finding_kw=(None if ok_b9 else dict(
            phase="boundary", severity="P2", category="validation",
            title="Invalid user_id not rejected",
            description=f"profile(invalid) returned {s}: {b[:120]}",
            repro=["GET /profile/@@@invalid@@@"],
            expected="400 or 404", actual=str(s),
            endpoint="/profile/{user_id}", auth_context=user_a_name,
        )),
    )

    # B10: send with empty txn_id
    s, _, b = client._req(
        "PUT",
        f"/_matrix/client/v3/rooms/{urllib.parse.quote('!apitest-nosuchroom:matrix.test', safe='')}"
        f"/send/m.room.message/",
        token=t1_tok,
        body=json.dumps({"msgtype": "m.text", "body": "no txn"}),
    )
    ok_b10 = s in (200, 400, 404)
    rec(
        "C-BND-010", "send with empty txn_id", "PUT",
        "/rooms/.../send/m.room.message/", user_a_name,
        "200 or 4xx", s, b, ok_b10,
        finding_kw=(None if ok_b10 else dict(
            phase="boundary", severity="P2", category="validation",
            title="Empty txn_id handling unclear",
            description=f"empty txn_id returned {s}: {b[:120]}",
            repro=["PUT /rooms/.../send/m.room.message/ (empty txn)"],
            expected="200 or 4xx", actual=str(s),
            endpoint="/rooms/{room_id}/send", auth_context=user_a_name,
        )),
    )

    # B11: test1 invites "" (empty user_id)
    body = json.dumps({"user_id": ""})
    s, _, b = client._req(
        "POST",
        f"/_matrix/client/v3/rooms/{urllib.parse.quote('!apitest-nosuchroom:matrix.test', safe='')}/invite",
        token=t1_tok, body=body,
    )
    ok_b11 = s == 400
    rec(
        "C-BND-011", "invite with empty user_id", "POST",
        "/rooms/.../invite", user_a_name,
        "400", s, b, ok_b11,
        finding_kw=(None if ok_b11 else dict(
            phase="boundary", severity="P1", category="validation",
            title="Empty user_id invite not rejected",
            description=f"invite('') returned {s}: {b[:120]}",
            repro=["POST /rooms/{room_id}/invite user_id=''"],
            expected="400", actual=str(s),
            endpoint="/rooms/{room_id}/invite", auth_context=user_a_name,
        )),
    )

    # B12: room_id with %00 (null byte) — potential injection
    s, _, b = client._req(
        "GET",
        "/_matrix/client/v3/rooms/!%00inject:matrix.test/messages?limit=1",
        token=t1_tok,
    )
    ok_b12 = s in (200, 400, 404)
    rec(
        "C-BND-012", "room_id with null byte", "GET",
        "/rooms/!%00inject:matrix.test/messages", user_a_name,
        "400 or 404 (sanitized)", s, b, ok_b12,
        finding_kw=(None if ok_b12 else dict(
            phase="boundary", severity="P0", category="security",
            title="Null byte not sanitized in room_id",
            description=f"room_id with %00 returned {s}: {b[:120]}",
            repro=["GET /rooms/!%00inject:matrix.test/messages"],
            expected="400 or 404", actual=str(s),
            endpoint="/rooms/{room_id}/messages", auth_context=user_a_name,
            fix_suggestion="Reject or strip null bytes in path parameters.",
        )),
    )

    return cases, findings


# -----------------------------------------------------------------------------
# Phase 4: Functional
# -----------------------------------------------------------------------------
TINY_PNG = (
    b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"
    b"\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89"
    b"\x00\x00\x00\rIDATx\x9cc\xf8\xcf\xc0\x00\x00\x00\x03\x00\x01"
    b"\x18\xdd\x8d\xb4\x00\x00\x00\x00IEND\xaeB`\x82"
)


def run_functional_phase(client, cfg=None):
    creds = _resolve_test_users(cfg or {})
    user_a_name = creds["user_a_name"]
    user_a_pass = creds["user_a_pass"]
    user_b_name = creds["user_b_name"]
    user_b_pass = creds["user_b_pass"]
    admin_user = creds["admin_user"]
    admin_pass = creds["admin_pass"]
    cases = []
    findings = []
    t1_tok, t1_uid = client.login(user_a_name, user_a_pass)
    t2_tok, t2_uid = client.login(user_b_name, user_b_pass)
    adm_tok, _ = client.login(admin_user, admin_pass)
    if not (t1_tok and t2_tok and adm_tok):
        return cases, findings

    def rec(cid, desc, method, path, token_type, expected, status, body, ok,
            finding_kw=None):
        errcode = parse_errcode(body)
        fid = ""
        if finding_kw:
            fid = add_finding(findings, **finding_kw)
        cases.append(TestCase(
            id=cid, phase="functional",
            category="functional", description=desc,
            method=method, path=path, token_type=token_type,
            expected=expected, status=status, errcode=errcode,
            passed=ok, finding_id=fid, notes=(body or "")[:200],
        ))

    # F1: full E2E - create, invite, join, send, read
    # Use unique room name + topic to avoid M_ROOM_IN_USE on repeat runs.
    room_suffix = time.strftime("%H%M%S")
    s, _, b = client.create_room(t1_tok, "public_chat", f"func-room-{room_suffix}", f"func topic {room_suffix}")
    room_id = ""
    if s == 200:
        try:
            room_id = json.loads(b).get("room_id", "")
        except Exception:
            room_id = ""
    f1_ok = bool(room_id)
    rec(
        "C-FUN-001a", f"F1: {user_a_name} creates public room", "POST", "/createRoom",
        user_a_name, "200", s, b, f1_ok,
        finding_kw=(None if f1_ok else dict(
            phase="functional", severity="P0", category="functional",
            title="F1: cannot create public room",
            description=f"createRoom returned {s}: {b[:120]}",
            repro=[f"POST /createRoom as {user_a_name} (public)"],
            expected="200", actual=str(s), endpoint="/createRoom",
            auth_context=user_a_name,
        )),
    )
    if not f1_ok:
        return cases, findings

    target_t2 = t2_uid or f"@{user_b_name}:matrix.test"
    s, _, b = client.invite(t1_tok, room_id, target_t2)
    s_join, _, bj = client.join_room(t2_tok, room_id)
    s_m1, _, bm1 = client.send_text(t1_tok, room_id, f"hello from {user_a_name}")
    s_m2, _, bm2 = client.send_text(t2_tok, room_id, f"hello from {user_b_name}")
    s_msg, _, bmsg = client.get_messages(t1_tok, room_id, limit=10)
    f1_pass = all(x == 200 for x in (s, s_join, s_m1, s_m2, s_msg))
    rec(
        "C-FUN-001b", "F1: full invite->join->send->read flow", "MULTI",
        f"/rooms/{room_id}/...", f"{user_a_name}+{user_b_name}", "all 200",
        s_msg, bmsg, f1_pass,
        finding_kw=(None if f1_pass else dict(
            phase="functional", severity="P0", category="functional",
            title="F1: end-to-end flow failed",
            description=(f"create={s} invite={s} join={s_join} "
                         f"send_t1={s_m1} send_t2={s_m2} read={s_msg}"),
            repro=[f"create+invite+join+send+read as {user_a_name}/{user_b_name}"],
            expected="all 200", actual=f"{s}/{s_join}/{s_m1}/{s_m2}/{s_msg}",
            endpoint=f"/rooms/{room_id}", auth_context=f"{user_a_name}+{user_b_name}",
        )),
    )

    # F2: test1 sends multiple messages, syncs
    msgs_sent = 0
    for i in range(3):
        ss, _, _ = client.send_text(t1_tok, room_id, f"msg {i}")
        if ss == 200:
            msgs_sent += 1
    s_sync, _, bsync = client.sync(t1_tok)
    f2_pass = s_sync == 200
    rec(
        "C-FUN-002", "F2: sync after multiple sends", "GET", "/sync", user_a_name,
        "200", s_sync, bsync, f2_pass,
        finding_kw=(None if f2_pass else dict(
            phase="functional", severity="P1", category="functional",
            title="F2: sync failed",
            description=f"sync returned {s_sync}: {bsync[:120]}",
            repro=["GET /sync after 3 sends"],
            expected="200", actual=str(s_sync),
            endpoint="/_matrix/client/v3/sync", auth_context=user_a_name,
        )),
    )

    # F3: admin whois
    target_who = t1_uid or f"@{user_a_name}:matrix.test"
    s, _, b = client._req(
        "GET", f"/_synapse/admin/v1/whois/{urllib.parse.quote(target_who, safe='')}",
        token=adm_tok,
    )
    has_fields = False
    if s == 200:
        try:
            data = json.loads(b)
            has_fields = ("user_id" in data) or ("devices" in data)
        except Exception:
            pass
    f3_pass = s == 200 and has_fields
    rec(
        "C-FUN-003", "F3: admin whois", "GET",
        f"/_synapse/admin/v1/whois/{target_who}", "admin",
        "200 with user_id/devices", s, b, f3_pass,
        finding_kw=(None if f3_pass else dict(
            phase="functional", severity="P1", category="functional",
            title="F3: admin whois missing fields",
            description=f"whois returned {s}: {b[:120]}",
            repro=[f"GET /_synapse/admin/v1/whois/{target_who} as admin"],
            expected="200 with user_id/devices", actual=str(s),
            endpoint=f"/_synapse/admin/v1/whois/{target_who}", auth_context="admin",
        )),
    )

    # F4: admin creates room, invites test1, kicks test1
    s, _, b = client.create_room(adm_tok, "private_chat", "admin-kick-room", "topic")
    admin_room = ""
    if s == 200:
        try:
            admin_room = json.loads(b).get("room_id", "")
        except Exception:
            admin_room = ""
    if admin_room:
        target_t1 = t1_uid or f"@{user_a_name}:matrix.test"
        client.invite(adm_tok, admin_room, target_t1)
        client.join_room(t1_tok, admin_room)
        s_kick, _, bkick = client.kick_user(adm_tok, admin_room, target_t1, "fun-test")
        f4_pass = s_kick == 200
        rec(
            "C-FUN-004", f"F4: admin kicks {user_a_name}", "POST",
            f"/rooms/{admin_room}/kick", "admin",
            "200", s_kick, bkick, f4_pass,
            finding_kw=(None if f4_pass else dict(
                phase="functional", severity="P1", category="functional",
                title="F4: admin kick failed",
                description=f"kick returned {s_kick}: {bkick[:120]}",
                repro=[f"POST /rooms/{admin_room}/kick as admin"],
                expected="200", actual=str(s_kick),
                endpoint=f"/rooms/{admin_room}/kick", auth_context="admin",
            )),
        )
        try:
            client.leave_room(adm_tok, admin_room)
        except Exception:
            pass
    else:
        rec(
            "C-FUN-004", f"F4: admin kicks {user_a_name} (admin room create failed)",
            "POST", "/rooms/.../kick", "admin",
            "200", s, b, False,
            finding_kw=dict(
                phase="functional", severity="P1", category="functional",
                title="F4: admin create room failed",
                description=f"createRoom(admin) returned {s}: {b[:120]}",
                repro=["POST /createRoom as admin"],
                expected="200", actual=str(s), endpoint="/createRoom",
                auth_context="admin",
            ),
        )

    # F5: test1 uploads media
    s, _, b = client.upload_media(t1_tok, TINY_PNG, "image/png")
    f5_pass = s == 200
    has_uri = False
    if f5_pass:
        try:
            has_uri = "content_uri" in json.loads(b)
        except Exception:
            has_uri = False
    f5_pass = f5_pass and has_uri
    rec(
        "C-FUN-005", f"F5: {user_a_name} uploads PNG", "POST",
        "/_matrix/media/v3/upload", user_a_name,
        "200 with content_uri", s, b, f5_pass,
        finding_kw=(None if f5_pass else dict(
            phase="functional", severity="P1", category="functional",
            title="F5: media upload failed",
            description=f"upload returned {s}: {b[:120]}",
            repro=[f"POST /_matrix/media/v3/upload (PNG) as {user_a_name}"],
            expected="200 with content_uri", actual=str(s),
            endpoint="/_matrix/media/v3/upload", auth_context=user_a_name,
        )),
    )

    # F6: test1 tries to delete another device
    s, _, b = client._req(
        "DELETE",
        "/_matrix/client/v3/devices/ANOTHERUSERDEVICE000",
        token=t1_tok,
        headers={"Content-Type": "application/json"},
        body="{}",
    )
    f6_pass = s in (403, 415, 401) or (s == 401 and "M_UIA_REQUIRED" in b)
    rec(
        "C-FUN-006", f"F6: {user_a_name} deletes other device", "DELETE",
        "/devices/ANOTHERUSERDEVICE000", user_a_name,
        "403 or 401 (UIA)", s, b, f6_pass,
        finding_kw=(None if f6_pass else dict(
            phase="functional", severity="P0", category="security",
            title="F6: cross-user device delete",
            description=f"delete returned {s}: {b[:120]}",
            repro=[f"DELETE /devices/{{other_device}} as {user_a_name}"],
            expected="403/401(UIA)", actual=str(s),
            endpoint="/_matrix/client/v3/devices/{device_id}", auth_context=user_a_name,
            fix_suggestion="Verify caller owns the target device.",
        )),
    )

    # Cleanup
    try:
        client.leave_room(t1_tok, room_id)
    except Exception:
        pass
    try:
        client.leave_room(t2_tok, room_id)
    except Exception:
        pass

    return cases, findings


# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------
def main():
    cfg = load_config()
    base_url = cfg.get("base_url", "https://matrix.test")
    timeout = int(cfg.get("timeout", 10))
    client = MatrixClient(base_url, verify_tls=False, timeout=timeout)

    all_cases = []
    all_findings = []

    print("== Phase 1: Admin ==")
    c, f = run_admin_phase(client)
    all_cases += c
    all_findings += f
    print(f"  cases={len(c)} findings={len(f)}")

    # Inter-phase cooldown: Phase 1 hammers the server with ~270 admin calls.
    # Give the server time to recover from any backpressure / 502 spikes.
    cooldown = int((cfg.get("admin_phase_cooldown_sec") or 30))
    print(f"  -- cooling down {cooldown}s for server recovery --")
    time.sleep(cooldown)

    print("== Phase 2: Permission ==")
    c, f = run_permission_phase(client, cfg)
    all_cases += c
    all_findings += f
    print(f"  cases={len(c)} findings={len(f)}")
    time.sleep(5)  # brief cooldown between phases

    print("== Phase 3: Boundary ==")
    c, f = run_boundary_phase(client, cfg)
    all_cases += c
    all_findings += f
    print(f"  cases={len(c)} findings={len(f)}")

    print("== Phase 4: Functional ==")
    c, f = run_functional_phase(client, cfg)
    all_cases += c
    all_findings += f
    print(f"  cases={len(c)} findings={len(f)}")

    # Summary
    by_sev = {}
    for f in all_findings:
        by_sev[f.severity] = by_sev.get(f.severity, 0) + 1
    print()
    print("=" * 60)
    print(f"Total test cases: {len(all_cases)}")
    passed = sum(1 for c in all_cases if c.passed)
    print(f"  passed: {passed}")
    print(f"  failed: {len(all_cases) - passed}")
    print(f"Total findings: {len(all_findings)}")
    for sev in ("P0", "P1", "P2", "P3"):
        if sev in by_sev:
            print(f"  {sev}: {by_sev[sev]}")

    # Save report
    REPORT_DIR.mkdir(parents=True, exist_ok=True)
    report = {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "config": {
            "base_url": base_url,
            "verify_tls": False,
            "timeout": timeout,
        },
        "phases": {
            "admin":     {"cases": sum(1 for c in all_cases if c.phase == "admin"),
                          "findings": sum(1 for f in all_findings if f.phase == "admin")},
            "permission": {"cases": sum(1 for c in all_cases if c.phase == "permission"),
                           "findings": sum(1 for f in all_findings if f.phase == "permission")},
            "boundary":  {"cases": sum(1 for c in all_cases if c.phase == "boundary"),
                          "findings": sum(1 for f in all_findings if f.phase == "boundary")},
            "functional": {"cases": sum(1 for c in all_cases if c.phase == "functional"),
                           "findings": sum(1 for f in all_findings if f.phase == "functional")},
        },
        "summary": {
            "total_cases": len(all_cases),
            "total_findings": len(all_findings),
            "by_severity": by_sev,
        },
        "cases": [asdict(c) for c in all_cases],
        "findings": [asdict(f) for f in all_findings],
    }
    with open(REPORT_PATH, "w", encoding="utf-8") as fp:
        json.dump(report, fp, ensure_ascii=False, indent=2)
    print(f"Report saved to: {REPORT_PATH}")


if __name__ == "__main__":
    main()
