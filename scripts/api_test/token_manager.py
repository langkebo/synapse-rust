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

import json
import time
import urllib.request
import urllib.error
from pathlib import Path
from typing import Optional

import yaml


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

    def _login(self, username: str, password: str) -> Optional[dict]:
        """调 /_matrix/client/r0/login 拿 access_token + user_id."""
        url = f"{self.base_url}/_matrix/client/r0/login"
        body = json.dumps(
            {
                "identifier": {"type": "m.id.user", "user": username},
                "password": password,
                "auth": {"type": "m.login.password"},
            }
        ).encode("utf-8")
        req = urllib.request.Request(url, data=body, method="POST")
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
        data = self._login(username, password)
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
