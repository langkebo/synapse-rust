"""
errcode_validator.py — 4xx errcode 验证规则 (Week 2 Task 3)

按 Matrix Client-Server API 规范,不同类型的端点在收到随机/畸形输入时,
应返回一组"合法"的标准 errcode。本模块按 (path 关键字, method) 维度声明
白名单,validate_errcode(path, method, errcode) 返回 True 表示该 errcode
在该端点类型下是合法的。

参考:
  https://spec.matrix.org/v1.10/client-server-api/#standard-error-codes
"""

from __future__ import annotations

import json
from typing import Optional

# =============================================================================
# Matrix 标准 errcode 全集
# =============================================================================
STANDARD_ERRCODES = {
    # 鉴权/通用
    "M_FORBIDDEN",
    "M_UNKNOWN_TOKEN",
    "M_MISSING_TOKEN",
    "M_UNAUTHORIZED",
    # 用户交互认证 (UIA)
    "M_UIA_REQUIRED",
    "M_USER_IN_USE",
    # 请求格式
    "M_BAD_JSON",
    "M_NOT_JSON",
    "M_TOO_LARGE",
    "M_PARTIAL_REQUEST",
    "M_MAX_UPLOAD_SIZE_EXCEEDED",
    # 参数
    "M_INVALID_ARGUMENT",
    "M_UNRECOGNIZED",
    "M_NOT_FOUND",
    # 用户/账号
    "M_USER_IN_USE",
    "M_USER_SUSPENDED",
    "M_INVALID_USERNAME",
    "M_INVALID_PASSWORD",
    "M_WEAK_PASSWORD",
    # 三方 ID
    "M_THREEPID_IN_USE",
    "M_THREEPID_NOT_FOUND",
    "M_3PID_INVALID",
    "M_3PID_DENIED",
    # 注册
    "M_REGISTRATION_DISABLED",
    "M_REGISTRATION_REQUIRED",
    # 房间
    "M_ROOM_IN_USE",
    "M_INVALID_ROOM_STATE",
    "M_INCOMPATIBLE_ROOM_STATE",
    "M_NOT_JOINED",
    "M_BAD_ALIAS",
    "M_UNSUPPORTED_ROOM_VERSION",
    "M_GUEST_ACCESS_FORBIDDEN",
    "M_CANNOT_LEAVE_SERVER_NOTICE_ROOM",
    # 密钥
    "M_WRONG_ROOM_KEYS_VERSION",
    "M_INVALID_SIGNATURE",
    "M_INVALID_DEVICE",
    "M_INVALID_DEVICE_ID",
    "M_NOT_YET_UPLOADED",
    # 配额/限流
    "M_LIMIT_EXCEEDED",
    "M_URL_NOT_SUPPORTED",
    "M_EXCLUSIVE",
    # 兜底
    "M_UNKNOWN",
}


# =============================================================================
# 通用基础 — 任何 authenticated 端点都可能产生的 errcode
# (鉴权 + 请求体格式 + 通用参数错误)
# =============================================================================
_BASE_AUTH = {
    "M_MISSING_TOKEN",
    "M_UNKNOWN_TOKEN",
    "M_UNAUTHORIZED",
    "M_FORBIDDEN",
    "M_BAD_JSON",
    "M_NOT_JSON",
    "M_INVALID_ARGUMENT",
    "M_UNRECOGNIZED",
    "M_UNKNOWN",
}

_BASE_OPTIONAL = {  # optional 端点可匿名访问,不该有 M_MISSING_TOKEN/M_UNKNOWN_TOKEN
    "M_FORBIDDEN",
    "M_UNAUTHORIZED",
    "M_BAD_JSON",
    "M_NOT_JSON",
    "M_INVALID_ARGUMENT",
    "M_UNRECOGNIZED",
    "M_UNKNOWN",
}

_POST_BODY = {
    "M_BAD_JSON",
    "M_NOT_JSON",
    "M_INVALID_ARGUMENT",
    "M_TOO_LARGE",
    "M_PARTIAL_REQUEST",
    "M_UNRECOGNIZED",
    "M_UNKNOWN",
}

_GET_NO_BODY = {  # GET 没有 body, 不会出 M_BAD_JSON
    "M_UNRECOGNIZED",
    "M_UNKNOWN",
}


# =============================================================================
# 端点分类规则
# =============================================================================
# 格式: list of (path_substring, method_set, additional_allowed_errcodes)
#
# 规则按"先具体,后通用"顺序排列;validate_errcode 取第一个匹配的规则,
# 把它的 allow set 与 _BASE 合并得到完整白名单。
# additional set 中若包含 _BASE 已有项则取并集,不会冲突。

_RULES: list[tuple[str, set[str], set[str]]] = [
    # ---- Login / 注册 (optional 鉴权弱) ----
    ("/login", {"GET"}, _BASE_OPTIONAL | {"M_UNRECOGNIZED"}),
    (
        "/login",
        {"POST"},
        _BASE_OPTIONAL
        | {
            "M_INVALID_USERNAME",
            "M_INVALID_PASSWORD",
            "M_USER_SUSPENDED",
            "M_LIMIT_EXCEEDED",
        },
    ),
    (
        "/register",
        {"GET", "POST"},
        _BASE_OPTIONAL
        | {
            "M_USER_IN_USE",
            "M_INVALID_USERNAME",
            "M_INVALID_PASSWORD",
            "M_WEAK_PASSWORD",
            "M_REGISTRATION_DISABLED",
            "M_THREEPID_IN_USE",
            "M_3PID_DENIED",
            "M_3PID_INVALID",
            "M_USER_SUSPENDED",
            "M_LIMIT_EXCEEDED",
        },
    ),
    (
        "/register/email/requestToken",
        {"POST"},
        _BASE_OPTIONAL
        | {
            "M_THREEPID_IN_USE",
            "M_3PID_INVALID",
            "M_LIMIT_EXCEEDED",
        },
    ),
    ("/register/email/submitToken", {"POST"}, _BASE_OPTIONAL | {"M_3PID_INVALID"}),
    ("/register/captcha", {"GET", "POST"}, _BASE_OPTIONAL),
    # ---- account 3pid 管理 (需 token; 敏感操作可能触发 UIA) ----
    (
        "/account/3pid/add",
        {"POST"},
        _BASE_AUTH
        | {
            "M_THREEPID_IN_USE",
            "M_3PID_INVALID",
            "M_UIA_REQUIRED",
        },
    ),
    (
        "/account/3pid/bind",
        {"POST"},
        _BASE_AUTH
        | {
            "M_THREEPID_IN_USE",
            "M_3PID_INVALID",
            "M_UIA_REQUIRED",
        },
    ),
    (
        "/account/3pid/delete",
        {"POST"},
        _BASE_AUTH
        | {
            "M_THREEPID_NOT_FOUND",
            "M_UIA_REQUIRED",
        },
    ),
    (
        "/account/3pid/unbind",
        {"POST"},
        _BASE_AUTH
        | {
            "M_THREEPID_NOT_FOUND",
            "M_3PID_INVALID",
            "M_UIA_REQUIRED",
        },
    ),
    (
        "/account/3pid",
        {"GET", "POST"},
        _BASE_AUTH
        | {
            "M_THREEPID_IN_USE",
            "M_THREEPID_NOT_FOUND",
            "M_3PID_INVALID",
            "M_LIMIT_EXCEEDED",
            "M_UIA_REQUIRED",
        },
    ),
    (
        "/account/password",
        {"POST"},
        _BASE_AUTH
        | {
            "M_INVALID_PASSWORD",
            "M_WEAK_PASSWORD",
            "M_THREEPID_NOT_FOUND",
            "M_3PID_INVALID",
            "M_LIMIT_EXCEEDED",
            "M_UIA_REQUIRED",
        },
    ),
    (
        "/account/deactivate",
        {"POST"},
        _BASE_AUTH | {"M_USER_SUSPENDED", "M_UIA_REQUIRED"},
    ),
    ("/account/whoami", {"GET"}, _BASE_AUTH),
    ("/account/guest/upgrade", {"POST"}, _BASE_AUTH),
    ("/account/guest", {"GET"}, _BASE_AUTH),
    (
        "/account/password/email",
        {"POST"},
        _BASE_AUTH
        | {
            "M_3PID_INVALID",
            "M_LIMIT_EXCEEDED",
        },
    ),
    # ---- Token 刷新 / Logout ----
    # logout 随机 body 触发 token 验证失败 → M_UNAUTHORIZED 或 M_MISSING_TOKEN
    ("/logout", {"POST"}, _BASE_AUTH | {"M_UIA_REQUIRED"}),
    (
        "/refresh",
        {"POST"},
        _BASE_AUTH | {"M_UIA_REQUIRED", "M_INVALID_ARGUMENT", "M_LIMIT_EXCEEDED"},
    ),
    # ---- 设备管理 ----
    ("/devices", {"GET"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    (
        "/delete_devices",
        {"POST"},
        _BASE_AUTH | {"M_INVALID_DEVICE_ID", "M_UIA_REQUIRED"},
    ),
    # ---- Push 通知 ----
    ("/push/devices", {"GET", "POST"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    ("/push/rules", {"GET", "POST"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    ("/pushers", {"GET", "POST"}, _BASE_AUTH),
    ("/pushrules", {"GET"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    # ---- 房间 (无 path param 的子集) ----
    ("/createRoom", {"POST"}, _BASE_AUTH | {"M_ROOM_IN_USE", "M_LIMIT_EXCEEDED"}),
    ("/create_dm", {"POST"}, _BASE_AUTH),
    ("/rooms/create_private", {"POST"}, _BASE_AUTH | {"M_ROOM_IN_USE"}),
    ("/rooms/typing", {"POST"}, _BASE_AUTH),
    # ---- Spaces (无 path param 子集) ----
    ("/spaces", {"GET", "POST"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    # ---- 搜索 ----
    ("/search", {"POST"}, _BASE_AUTH),
    ("/search_recipients", {"POST"}, _BASE_AUTH),
    ("/search_rooms", {"POST"}, _BASE_AUTH),
    ("/user_directory", {"POST"}, _BASE_AUTH | {"M_LIMIT_EXCEEDED"}),
    # ---- 密钥管理 / E2EE ----
    ("/keys/claim", {"POST"}, _BASE_AUTH | {"M_INVALID_DEVICE"}),
    ("/keys/upload", {"POST"}, _BASE_AUTH),
    ("/keys/query", {"POST"}, _BASE_AUTH),
    ("/keys/signatures", {"POST"}, _BASE_AUTH | {"M_INVALID_SIGNATURE"}),
    ("/keys/signatures/upload", {"POST"}, _BASE_AUTH | {"M_INVALID_SIGNATURE"}),
    ("/keys/changes", {"GET"}, _BASE_AUTH),
    (
        "/keys/device_signing",
        {"GET", "POST", "PUT"},
        _BASE_AUTH
        | {
            "M_INVALID_SIGNATURE",
            "M_UIA_REQUIRED",
        },
    ),
    ("/keys/qr_code", {"GET", "POST"}, _BASE_AUTH),
    ("/keys/verification", {"POST"}, _BASE_AUTH),
    ("/keys/rotation", {"GET", "POST", "PUT"}, _BASE_AUTH),
    ("/keys/backup", {"GET", "POST"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    # ---- 房间 GET (rooms/{room_id}/...) — 不存在的房间返回 M_NOT_FOUND ----
    ("/rooms/", {"GET"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    # ---- Room Keys 备份 (有 path-param 子集不在白名单) ----
    (
        "/room_keys",
        {"GET", "POST", "PUT", "DELETE"},
        _BASE_AUTH
        | {
            "M_NOT_FOUND",
            "M_WRONG_ROOM_KEYS_VERSION",
        },
    ),
    # ---- VoIP ----
    ("/voip", {"GET", "POST"}, _BASE_AUTH),
    # ---- Voice / 语音 (含 upload, 上传可能 M_TOO_LARGE) ----
    (
        "/voice/upload",
        {"POST"},
        _BASE_AUTH | {"M_TOO_LARGE", "M_MAX_UPLOAD_SIZE_EXCEEDED"},
    ),
    ("/voice/config", {"GET"}, _BASE_AUTH),
    ("/voice/stats", {"GET"}, _BASE_AUTH),
    # ---- Media ----
    ("/media/config", {"GET"}, _BASE_AUTH),
    (
        "/media/preview_url",
        {"GET"},
        _BASE_AUTH | {"M_URL_NOT_SUPPORTED", "M_NOT_FOUND"},
    ),
    # ---- Direct / Friends / Spaces 子项 (无 path-param) ----
    ("/direct", {"GET", "POST"}, _BASE_AUTH),
    ("/joined_rooms", {"GET"}, _BASE_AUTH),
    ("/my_rooms", {"GET"}, _BASE_AUTH),
    ("/events", {"GET"}, _BASE_AUTH),
    ("/notifications", {"GET"}, _BASE_AUTH),
    ("/sync", {"GET", "POST"}, _BASE_AUTH),
    ("/threads", {"GET", "POST"}, _BASE_AUTH),
    ("/presence", {"POST"}, _BASE_AUTH),
    ("/friendships", {"GET", "POST"}, _BASE_AUTH),
    ("/friends", {"GET", "POST"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    ("/appservice", {"GET"}, _BASE_AUTH),
    ("/device_trust", {"GET"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    ("/device_verification", {"POST"}, _BASE_AUTH),
    ("/security/summary", {"GET"}, _BASE_AUTH),
    ("/translate", {"POST"}, _BASE_AUTH),
    ("/widgets", {"POST"}, _BASE_AUTH),
    ("/external_services", {"GET"}, _BASE_AUTH),
    ("/user/burn", {"GET", "POST", "PUT"}, _BASE_AUTH),
    ("/auth_metadata", {"GET"}, _BASE_AUTH),
    ("/config/client", {"GET"}, _BASE_AUTH),
    ("/rendezvous", {"POST"}, _BASE_AUTH),
    ("/version", {"GET"}, _BASE_AUTH),
    ("/capabilities", {"GET"}, _BASE_OPTIONAL),
    ("/versions", {"GET"}, _BASE_OPTIONAL),
    ("/saml", {"GET"}, _BASE_OPTIONAL),
    ("/logout/saml", {"GET"}, _BASE_OPTIONAL),
    ("/publicRooms", {"GET", "POST"}, _BASE_OPTIONAL),
    ("/thirdparty/protocol/", {"GET"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    ("/thirdparty", {"GET"}, _BASE_OPTIONAL | {"M_NOT_FOUND"}),
    ("/login/sso", {"GET", "POST"}, _BASE_OPTIONAL),
    ("/login/saml", {"GET", "POST"}, _BASE_OPTIONAL),
    # ---- 目录查询 (directory/) ----
    ("/directory/list/room/", {"GET"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    ("/directory/room/", {"GET"}, _BASE_AUTH | {"M_NOT_FOUND", "M_BAD_ALIAS"}),
    # ---- 设备 / device trust ----
    ("/devices/", {"GET"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    ("/device_trust/", {"GET"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    # ---- friends (HuLa extended) ----
    # ---- 房间级 (rooms/{room_id}/...) ----
    # GET /rooms/{room_id}/... 返回 M_NOT_FOUND 当 room 不存在是合法行为
    ("/rooms/", {"GET"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    # ---- 密钥备份 (keys/backup/...) ----
    (
        "/keys/backup/secure/",
        {"GET", "POST", "PUT", "DELETE"},
        _BASE_AUTH | {"M_NOT_FOUND"},
    ),
    ("/keys/backup/", {"GET"}, _BASE_AUTH | {"M_NOT_FOUND"}),
    # ---- voice (HuLa extended) — 未实现时返回 M_UNRECOGNIZED ----
    ("/voice/", {"GET", "POST"}, _BASE_AUTH | {"M_NOT_FOUND", "M_UNRECOGNIZED"}),
]


# =============================================================================
# 校验入口
# =============================================================================
def _find_rule(path: str, method: str) -> Optional[set[str]]:
    """返回 (path, method) 命中的允许 errcode 集合;找不到返回 None.

    匹配规则: keyword 必须作为完整 path segment 出现 (即 keyword 后面必须是
    '/'、'{' (path param 前缀) 或 path 末尾)。这避免 "/direct" 错误命中
    "/directory/list/room/..."，也正确命中 "/devices/{device_id}".
    """
    method = method.upper()
    for keyword, methods, allowed in _RULES:
        if method not in methods:
            continue
        idx = path.find(keyword)
        while idx >= 0:
            after = idx + len(keyword)
            if (
                after == len(path) or path[after] in "/{"
            ):  # segment boundary or path-param start
                return allowed
            idx = path.find(keyword, idx + 1)
    return None


def validate_errcode(path: str, method: str, errcode: str | None) -> dict:
    """判定 errcode 是否合法.

    Returns:
        dict:
            - expected (set[str] | None): 该端点允许的 errcode 集合;None 表示无规则
            - valid (bool): errcode 是否在允许集合内
            - reason (str): 当 invalid 时的原因描述

    Note: errcode=None 表示响应不是 JSON (e.g. text/plain)，
          这通常是服务器对非预期输入的结构性错误 (400/500 with plain text)。
          对于 authenticated 端点,这类响应是合法的,不标记为 unexpected。
    """
    # errcode=None 表示非 JSON 响应,这类情况通常是合法的结构错误
    if errcode is None or errcode == "unparseable":
        return {"expected": None, "valid": True, "reason": ""}

    rule = _find_rule(path, method)
    if rule is None:
        # 没找到规则 — 退到标准 errcode 全集
        valid = errcode in STANDARD_ERRCODES
        return {
            "expected": STANDARD_ERRCODES,
            "valid": valid,
            "reason": (
                ""
                if valid
                else f"errcode {errcode} is not a standard Matrix errcode (no specific rule for {method} {path})"
            ),
        }

    valid = errcode in rule
    return {
        "expected": sorted(rule),
        "valid": valid,
        "reason": (
            ""
            if valid
            else f"errcode {errcode} is not allowed for {method} {path} (allowed: {sorted(rule)})"
        ),
    }


def summarize_expected_errcodes() -> dict:
    """返回本模块覆盖的规则统计 — 用于报告."""
    by_method: dict[str, int] = {}
    for _, methods, _ in _RULES:
        for m in methods:
            by_method[m] = by_method.get(m, 0) + 1
    return {
        "total_rules": len(_RULES),
        "rules_by_method": by_method,
        "standard_errcode_count": len(STANDARD_ERRCODES),
    }


if __name__ == "__main__":
    if len(sys.argv) >= 4:
        # CLI: python3 errcode_validator.py <path> <method> <errcode>
        p, m, e = sys.argv[1], sys.argv[2], sys.argv[3]
        result = validate_errcode(p, m, e)
        print(json.dumps(result, indent=2, ensure_ascii=False))
    else:
        # 默认打印规则统计
        print(json.dumps(summarize_expected_errcodes(), indent=2))
