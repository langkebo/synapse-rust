#!/usr/bin/env python3
"""
token_manager.py — Matrix Access Token 管理器 (Week 2 Task 2)

职责:
  - 从 config.yaml 读取 user/admin 凭据
  - 调 /_matrix/client/r0/login 拿 user + admin token
  - 缓存 token 直到过期
  - 提供 get_user_token() / get_admin_token() 接口

用法:
    from token_manager import TokenManager
    tm = TokenManager(base_url="http://localhost:8008", config_path="scripts/api_test/config.yaml")
    user_token = tm.get_user_token()
    admin_token = tm.get_admin_token()
"""

from __future__ import annotations

import hashlib
import hmac
import json
import os
import struct
import time
import urllib.request
import urllib.error
from pathlib import Path
from typing import Optional

import yaml


# admin 部署默认强制 MFA（ADMIN_MFA_REQUIRED=true）：admin 登录需附带当前 TOTP。
# 密钥来源优先级：环境变量 API_TEST_ADMIN_MFA_SECRET > config.admin.mfa_secret
#              > docker/deploy/.env 的 ADMIN_MFA_SHARED_SECRET
_ADMIN_MFA_SECRET_ENV = "API_TEST_ADMIN_MFA_SECRET"
_DEPLOY_ENV_FILE = Path(__file__).resolve().parents[2] / "docker" / "deploy" / ".env"


def _load_admin_mfa_secret(cfg: dict) -> Optional[str]:
    """定位 admin MFA 共享密钥（base32）。"""
    secret = os.environ.get(_ADMIN_MFA_SECRET_ENV) or ""
    if not secret:
        secret = (cfg.get("admin") or {}).get("mfa_secret") or ""
    if not secret and _DEPLOY_ENV_FILE.exists():
        for line in _DEPLOY_ENV_FILE.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line.startswith("ADMIN_MFA_SHARED_SECRET="):
                secret = line.split("=", 1)[1].strip().strip('"').strip("'")
                break
    return secret.strip() or None


def _decode_base32_secret(secret: str) -> bytes:
    """复刻服务端 decode_base32_secret：忽略空格与 -，= 截断，非法字符则回退原始字节。"""
    bits = 0
    bit_count = 0
    output = bytearray()
    for ch in secret:
        if ch in (" ", "-"):
            continue
        if ch == "=":
            break
        up = ch.upper()
        if "A" <= up <= "Z":
            value = ord(up) - ord("A")
        elif "2" <= ch <= "7":
            value = (ord(ch) - ord("2")) + 26
        else:
            return secret.encode("utf-8")
        bits = (bits << 5) | value
        bit_count += 5
        while bit_count >= 8:
            bit_count -= 8
            output.append((bits >> bit_count) & 0xFF)
            bits &= (1 << bit_count) - 1
    if not output:
        return secret.encode("utf-8")
    return bytes(output)


def _generate_totp(secret: str) -> str:
    """按 RFC 6238 生成 6 位 TOTP（HMAC-SHA1、30s 步长），与服务端算法一致。"""
    key = _decode_base32_secret(secret)
    step = int(time.time() // 30)
    digest = hmac.new(key, struct.pack(">Q", step), hashlib.sha1).digest()
    offset = digest[19] & 0x0F
    binary = (
        ((digest[offset] & 0x7F) << 24)
        | (digest[offset + 1] << 16)
        | (digest[offset + 2] << 8)
        | digest[offset + 3]
    )
    return f"{binary % 1_000_000:06d}"


class TokenManager:
    """管理 user/admin access token. 缓存以减少登录请求."""

    def __init__(
        self,
        base_url: str,
        config_path: str | Path = "scripts/api_test/config.yaml",
        verify_tls: bool | None = None,
    ):
        """Args:
        base_url: homeserver URL
        config_path: config.yaml path
        verify_tls: override config verify_tls (None=use config)
        """
        self.base_url = base_url.rstrip("/")
        self.config_path = Path(config_path)
        self._user_token: Optional[str] = None
        self._user_token_time: float = 0
        self._admin_token: Optional[str] = None
        self._admin_token_time: float = 0
        self._config = self._load_config()
        if verify_tls is None:
            verify_tls = bool(self._config.get("verify_tls", True))
        self._verify_tls = verify_tls
        # Build SSL context once
        if self._verify_tls:
            self._ssl_ctx = None  # use default
        else:
            import ssl

            self._ssl_ctx = ssl._create_unverified_context()

    def _load_config(self) -> dict:
        if not self.config_path.exists():
            return {
                "auth": {"username": "testuser1", "password": "Test@123"},
                "admin": {"username": "admin", "password": "Admin@123"},
            }
        with open(self.config_path) as f:
            return yaml.safe_load(f) or {}

    def _login(
        self, username: str, password: str, mfa_code: Optional[str] = None
    ) -> Optional[dict]:
        """调 /_matrix/client/r0/login 拿 access_token + user_id."""
        url = f"{self.base_url}/_matrix/client/r0/login"
        body: dict = {
            "identifier": {"type": "m.id.user", "user": username},
            "password": password,
            "auth": {"type": "m.login.password"},
        }
        if mfa_code:
            body["mfa_code"] = mfa_code
        req = urllib.request.Request(
            url, data=json.dumps(body).encode("utf-8"), method="POST"
        )
        req.add_header("Content-Type", "application/json")
        try:
            with urllib.request.urlopen(req, timeout=10, context=self._ssl_ctx) as resp:
                data = json.loads(resp.read().decode())
                if "access_token" in data:
                    return data
        except urllib.error.HTTPError as e:
            err_body = e.read().decode()[:200]
            print(f"[token] login FAILED for {username}: {e.code} {err_body}")
        except Exception as e:
            print(f"[token] login ERROR for {username}: {e}")
        return None

    def get_user_token(self, force_refresh: bool = False) -> Optional[str]:
        """获取 user token. 缓存 50 分钟 (Matrix tokens 默认无过期,但保守刷新)."""
        if (
            not force_refresh
            and self._user_token
            and (time.time() - self._user_token_time) < 3000
        ):
            return self._user_token
        creds = self._config.get("auth", {})
        username = creds.get("username", "testuser1")
        password = creds.get("password", "Test@123")
        data = self._login(username, password)
        if data:
            self._user_token = data["access_token"]
            self._user_token_time = time.time()
            print(f"[token] user token obtained for {data.get('user_id', '?')}")
        return self._user_token

    def get_admin_token(self, force_refresh: bool = False) -> Optional[str]:
        """获取 admin token."""
        if (
            not force_refresh
            and self._admin_token
            and (time.time() - self._admin_token_time) < 3000
        ):
            return self._admin_token
        creds = self._config.get("admin", {})
        username = creds.get("username", "admin")
        password = creds.get("password", "Admin@123")
        # admin 强制 MFA 时登录需附带当前 TOTP；无密钥则不附带（服务器将按未启用处理）
        mfa_secret = _load_admin_mfa_secret(self._config)
        mfa_code = _generate_totp(mfa_secret) if mfa_secret else None
        data = self._login(username, password, mfa_code=mfa_code)
        if data:
            self._admin_token = data["access_token"]
            self._admin_token_time = time.time()
            print(f"[token] admin token obtained for {data.get('user_id', '?')}")
        return self._admin_token

    def auth_header(self, token_type: str = "user") -> dict[str, str]:
        """返回 Authorization header dict."""
        token = (
            self.get_user_token() if token_type == "user" else self.get_admin_token()
        )
        if not token:
            return {}
        return {"Authorization": f"Bearer {token}"}


if __name__ == "__main__":
    import argparse

    ap = argparse.ArgumentParser(description="Test token manager")
    ap.add_argument("--base-url", default="http://localhost:8008")
    ap.add_argument("--config", default="scripts/api_test/config.yaml")
    args = ap.parse_args()

    tm = TokenManager(base_url=args.base_url, config_path=args.config)
    user_token = tm.get_user_token()
    admin_token = tm.get_admin_token()
    print(
        f"User token:  {user_token[:20] if user_token else 'NONE'}... len={len(user_token) if user_token else 0}"
    )
    print(
        f"Admin token: {admin_token[:20] if admin_token else 'NONE'}... len={len(admin_token) if admin_token else 0}"
    )
