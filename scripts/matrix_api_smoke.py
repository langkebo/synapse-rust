#!/usr/bin/env python3

import argparse
import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request


def api_request(base_url, method, path, body=None, token=None, timeout=30):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"

    data = None
    if body is not None:
        data = json.dumps(body).encode("utf-8")

    req = urllib.request.Request(
        f"{base_url}{path}",
        data=data,
        headers=headers,
        method=method,
    )

    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read().decode("utf-8")
            return resp.status, json.loads(raw) if raw else {}
    except urllib.error.HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        try:
            payload = json.loads(raw) if raw else {}
        except json.JSONDecodeError:
            payload = {"raw": raw}
        raise RuntimeError(
            f"HTTP {exc.code} for {method} {path}: {json.dumps(payload, ensure_ascii=False)}"
        ) from exc


def register_user(base_url, username, password):
    status, payload = api_request(
        base_url,
        "POST",
        "/_matrix/client/v3/register",
        {
            "auth": {"type": "m.login.dummy"},
            "username": username,
            "password": password,
        },
    )
    if status != 200:
        raise RuntimeError(f"register failed for {username}: {payload}")
    return payload


def login_user(base_url, username, password):
    status, payload = api_request(
        base_url,
        "POST",
        "/_matrix/client/v3/login",
        {
            "type": "m.login.password",
            "user": username,
            "password": password,
        },
    )
    if status != 200:
        raise RuntimeError(f"login failed for {username}: {payload}")
    return payload


def wait_for_message(base_url, token, room_id, expected_body, attempts, interval_seconds):
    for _ in range(attempts):
        status, payload = api_request(
            base_url,
            "GET",
            "/_matrix/client/v3/sync?timeout=1000",
            token=token,
        )
        if status != 200:
            raise RuntimeError(f"sync failed: {payload}")

        timeline = (
            payload.get("rooms", {})
            .get("join", {})
            .get(room_id, {})
            .get("timeline", {})
            .get("events", [])
        )
        for event in timeline:
            if (
                event.get("type") == "m.room.message"
                and event.get("content", {}).get("body") == expected_body
            ):
                return payload
        time.sleep(interval_seconds)

    raise RuntimeError("message not observed in receiver sync timeline")


def main():
    parser = argparse.ArgumentParser(description="Matrix API smoke test")
    parser.add_argument(
        "--base-url",
        default="http://127.0.0.1:28080",
        help="Base URL for the Matrix client API",
    )
    parser.add_argument(
        "--password",
        default="SmokePass@123",
        help="Password used for newly registered smoke users",
    )
    parser.add_argument(
        "--sync-attempts",
        type=int,
        default=6,
        help="Number of sync attempts to observe the sent message",
    )
    parser.add_argument(
        "--sync-interval-seconds",
        type=float,
        default=1.0,
        help="Delay between sync attempts",
    )
    args = parser.parse_args()

    suffix = str(int(time.time()))
    user1 = f"smoke_{suffix}_a"
    user2 = f"smoke_{suffix}_b"
    message = f"smoke message {suffix}"

    register_user(args.base_url, user1, args.password)
    register_user(args.base_url, user2, args.password)

    login1 = login_user(args.base_url, user1, args.password)
    login2 = login_user(args.base_url, user2, args.password)

    status, room_resp = api_request(
        args.base_url,
        "POST",
        "/_matrix/client/v3/createRoom",
        {"name": f"Smoke Room {suffix}", "preset": "private_chat"},
        token=login1["access_token"],
    )
    if status != 200:
        raise RuntimeError(f"createRoom failed: {room_resp}")

    room_id = room_resp["room_id"]
    room_id_enc = urllib.parse.quote(room_id, safe="")

    status, invite_resp = api_request(
        args.base_url,
        "POST",
        f"/_matrix/client/v3/rooms/{room_id_enc}/invite",
        {"user_id": login2["user_id"]},
        token=login1["access_token"],
    )
    if status != 200:
        raise RuntimeError(f"invite failed: {invite_resp}")

    join_target = urllib.parse.quote(room_id, safe="")
    status, join_resp = api_request(
        args.base_url,
        "POST",
        f"/_matrix/client/v3/join/{join_target}",
        {},
        token=login2["access_token"],
    )
    if status != 200:
        raise RuntimeError(f"join failed: {join_resp}")

    txn_id = f"smoke-{suffix}"
    status, send_resp = api_request(
        args.base_url,
        "PUT",
        f"/_matrix/client/v3/rooms/{room_id_enc}/send/m.room.message/{txn_id}",
        {"msgtype": "m.text", "body": message},
        token=login1["access_token"],
    )
    if status != 200:
        raise RuntimeError(f"send failed: {send_resp}")

    wait_for_message(
        args.base_url,
        login2["access_token"],
        room_id,
        message,
        args.sync_attempts,
        args.sync_interval_seconds,
    )

    result = {
        "base_url": args.base_url,
        "user1": login1["user_id"],
        "user2": login2["user_id"],
        "room_id": room_id,
        "event_id": send_resp["event_id"],
        "message": message,
        "result": "ok",
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(1)
