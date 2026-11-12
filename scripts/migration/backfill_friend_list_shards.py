#!/usr/bin/env python3
"""
B1 Step 3: backfill legacy `m.friends.list` entries into W5 shards.

Before the B1 fix, every friend was appended to the legacy state event
(`state_key = ''`) because `read_friend_shard_for_update` unconditionally fell
back to `''`. Reads were also legacy-only. Both paths are fixed, so *new*
friends land in their per-letter shard, but pre-existing friends stay in `''`
until this script moves them.

For every friend-list room with a non-empty legacy list, the script:

  1. reads the latest `state_key = ''` content (the legacy list),
  2. reads the latest content of every non-empty shard,
  3. merges each legacy friend into the shard derived from its user_id
     (`shard_for_user_id`, mirroring `synapse_common::friend_shard`),
  4. writes one new `m.friends.list` event per touched shard,
  5. rewrites `state_key = ''` with an empty list (keeps the room's marker
     event, which `create_friend_list_room` always creates).

The write columns/format match `FriendRoomStorage::save_friend_groups`
(`event_id = "$<uuid32>:localhost"`, `depth = 1`) — the same direct-insert
pattern the storage layer already uses for vendor friend state events.

Idempotent: after step 5 the legacy list is empty, so a second run skips the
room. Run it with the app stopped (or at least strictly read-only for friends)
to avoid a lost update against a concurrent `add_friend`.

Usage:
    export DATABASE_URL=postgresql://user:pass@host:port/db
    python3 scripts/migration/backfill_friend_list_shards.py --dry-run  # preview
    python3 scripts/migration/backfill_friend_list_shards.py            # execute
    python3 scripts/migration/backfill_friend_list_shards.py --verify   # invariants

Exit code is non-zero when an invariant fails.
"""

import argparse
import asyncio
import json
import os
import sys
import time
import uuid
from typing import Any, Dict, List, Optional, Tuple

import asyncpg

try:
    from dotenv import load_dotenv

    load_dotenv()
except ImportError:
    pass

DEFAULT_DATABASE_URL = os.getenv("DATABASE_URL")

EVENT_TYPE = "m.friends.list"


# ── Routing primitives (must match synapse_common::friend_shard) ──────────────


def _localpart(user_id: str) -> str:
    """`@local:server` -> `local`; anything else is used verbatim (Rust parity)."""
    if user_id.startswith("@"):
        return user_id[1:].split(":", 1)[0]
    return user_id


def sort_letter(value: str) -> str:
    for ch in value:
        if not ch.isspace():
            return ch.upper() if ch.isascii() and ch.isalpha() else "#"
    return "#"


def shard_state_key(friend_id: str) -> str:
    return sort_letter(_localpart(friend_id))


# ── Queries ──────────────────────────────────────────────────────────────────


LATEST_LEGACY_SQL = """
SELECT DISTINCT ON (e.room_id)
       e.room_id, e.sender, e.content, e.origin_server_ts
FROM events e
WHERE e.event_type = $1
  AND e.state_key = ''
  AND COALESCE(e.soft_failed, FALSE) = FALSE
ORDER BY e.room_id, e.origin_server_ts DESC, e.stream_ordering DESC
"""

LATEST_SHARDS_SQL = """
SELECT DISTINCT ON (e.state_key)
       e.state_key, e.content, e.origin_server_ts
FROM events e
WHERE e.room_id = $1
  AND e.event_type = $2
  AND e.state_key <> ''
  AND COALESCE(e.soft_failed, FALSE) = FALSE
ORDER BY e.state_key, e.origin_server_ts DESC, e.stream_ordering DESC
"""

INSERT_SQL = """
INSERT INTO events (event_id, room_id, sender, event_type, state_key, content, origin_server_ts, depth)
VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7, 1)
"""


def _friends(content: Any) -> List[Dict[str, Any]]:
    if isinstance(content, str):
        content = json.loads(content)
    if not isinstance(content, dict):
        return []
    friends = content.get("friends")
    return (
        [f for f in friends if isinstance(f, dict)] if isinstance(friends, list) else []
    )


def _version(content: Any) -> int:
    if isinstance(content, str):
        content = json.loads(content)
    if isinstance(content, dict) and isinstance(content.get("version"), int):
        return content["version"]
    return 1


def _dedupe(friends: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
    seen = set()
    out = []
    for friend in friends:
        user_id = friend.get("user_id")
        if not isinstance(user_id, str) or not user_id or user_id in seen:
            continue
        seen.add(user_id)
        out.append(friend)
    return out


def _event_id() -> str:
    return f"${uuid.uuid4().hex}:localhost"


def plan_room(
    room_id: str,
    sender: str,
    legacy_content: Any,
    shards: List[Tuple[str, Any]],
    now_ms: int,
) -> Tuple[List[Tuple[str, Dict[str, Any]]], set]:
    """Return `[(new_state_key, new_content)]` and the expected friend-id set."""
    if isinstance(legacy_content, str):
        legacy_content = json.loads(legacy_content)
    legacy_content = legacy_content if isinstance(legacy_content, dict) else {}

    legacy_friends = _dedupe(_friends(legacy_content))
    if not legacy_friends:
        return [], set()

    existing: Dict[str, Dict[str, Any]] = {}
    for state_key, content in shards:
        if isinstance(content, str):
            content = json.loads(content)
        existing[state_key] = content if isinstance(content, dict) else {}

    grouped: Dict[str, List[Dict[str, Any]]] = {}
    for friend in legacy_friends:
        grouped.setdefault(shard_state_key(friend["user_id"]), []).append(friend)

    legacy_version = _version(legacy_content)
    writes: List[Tuple[str, Dict[str, Any]]] = []
    expected = {f["user_id"] for f in legacy_friends}
    for content in existing.values():
        expected.update(f["user_id"] for f in _friends(content))

    for state_key, incoming in sorted(grouped.items()):
        current = existing.get(state_key, {})
        merged = _dedupe(_friends(current) + incoming)

        if current:
            content = dict(current)
        else:
            content = {k: v for k, v in legacy_content.items() if k != "friends"}
        content["friends"] = merged
        content["version"] = max(_version(current), legacy_version) + 1
        if "updated_ts" in content:
            content["updated_ts"] = now_ms
        writes.append((state_key, content))

    legacy_new = {k: v for k, v in legacy_content.items() if k != "friends"}
    legacy_new["friends"] = []
    legacy_new["version"] = legacy_version + 1
    if "updated_ts" in legacy_new:
        legacy_new["updated_ts"] = now_ms
    writes.append(("", legacy_new))

    return writes, expected


async def process_room(
    pool: asyncpg.Pool, row: asyncpg.Record, now_ms: int
) -> Tuple[int, int]:
    room_id = row["room_id"]
    sender = row["sender"]
    legacy_content = row["content"]

    async with pool.acquire() as conn:
        async with conn.transaction():
            shards = await conn.fetch(LATEST_SHARDS_SQL, room_id, EVENT_TYPE)
            writes, expected = plan_room(
                room_id,
                sender,
                legacy_content,
                [(r["state_key"], r["content"]) for r in shards],
                now_ms,
            )
            if not writes:
                return 0, 0

            for state_key, content in writes:
                await conn.execute(
                    INSERT_SQL,
                    _event_id(),
                    room_id,
                    sender,
                    EVENT_TYPE,
                    state_key,
                    json.dumps(content, separators=(",", ":"), ensure_ascii=False),
                    now_ms,
                )

            # Post-write invariant: the union of all shard friends must equal what the
            # pre-write state (legacy ∪ existing shards) contained. Guards against a
            # merge bug dropping an entry.
            after = await conn.fetch(LATEST_SHARDS_SQL, room_id, EVENT_TYPE)
            actual = set()
            for shard in after:
                actual.update(f["user_id"] for f in _friends(shard["content"]))
            if actual != expected:
                raise RuntimeError(
                    f"invariant failed for {room_id}: expected {len(expected)} friends, got {len(actual)} "
                    f"(missing={sorted(expected - actual)[:5]}, extra={sorted(actual - expected)[:5]})"
                )

    moved = len(expected)
    return moved, len([k for k, _ in writes if k])


async def run(database_url: str, dry_run: bool, limit: Optional[int]) -> int:
    pool = await asyncpg.create_pool(database_url, min_size=1, max_size=4)
    try:
        rows = await pool.fetch(LATEST_LEGACY_SQL, EVENT_TYPE)
        candidates = [r for r in rows if _friends(r["content"])]
        if limit is not None:
            candidates = candidates[:limit]

        total_legacy = sum(len(_friends(r["content"])) for r in candidates)
        print(
            f"Found {len(candidates)} friend-list room(s) with legacy friends "
            f"({total_legacy} entries across {len(rows)} legacy rows)"
        )
        if dry_run:
            for row in candidates:
                friends = _dedupe(_friends(row["content"]))
                keys = sorted({shard_state_key(f["user_id"]) for f in friends})
                print(
                    f"  {row['room_id']}: {len(friends)} friend(s) -> shards {''.join(keys) or '-'}"
                )
            print(
                f"Dry run: would redistribute {total_legacy} friend(s). No writes performed."
            )
            return 0

        now_ms = int(time.time() * 1000)
        moved = 0
        touched_shards = 0
        for index, row in enumerate(candidates, 1):
            room_moved, shard_count = await process_room(pool, row, now_ms + index)
            moved += room_moved
            touched_shards += shard_count
            print(
                f"  [{index}/{len(candidates)}] {row['room_id']}: moved {room_moved} friend(s), {shard_count} shard(s)"
            )

        print(
            f"Summary: redistributed {moved} friend(s) into {touched_shards} shard event(s) for {len(candidates)} room(s)."
        )
        return 0
    finally:
        await pool.close()


async def verify(database_url: str) -> int:
    pool = await asyncpg.create_pool(database_url, min_size=1, max_size=4)
    failures = 0
    try:
        rows = await pool.fetch(LATEST_LEGACY_SQL, EVENT_TYPE)
        non_empty = [r for r in rows if _friends(r["content"])]
        print(f"Legacy rows: {len(rows)}, still non-empty: {len(non_empty)}")
        for row in non_empty:
            failures += 1
            print(
                f"  FAIL {row['room_id']}: legacy '' still holds {len(_friends(row['content']))} friend(s)"
            )

        rooms = await pool.fetch(
            """
            SELECT DISTINCT room_id
            FROM events
            WHERE event_type = $1 AND COALESCE(soft_failed, FALSE) = FALSE
            """,
            EVENT_TYPE,
        )
        misplaced = 0
        total = 0
        for room in rooms:
            shards = await pool.fetch(LATEST_SHARDS_SQL, room["room_id"], EVENT_TYPE)
            for shard in shards:
                if shard["state_key"] == "":
                    continue
                for friend in _friends(shard["content"]):
                    total += 1
                    if shard_state_key(friend["user_id"]) != shard["state_key"]:
                        misplaced += 1
                        if misplaced <= 10:
                            print(
                                f"  FAIL {room['room_id']}: {friend['user_id']} sits in "
                                f"'{shard['state_key']}', expected '{shard_state_key(friend['user_id'])}'"
                            )
        print(f"Sharded friends: {total}, misplaced: {misplaced}")
        failures += misplaced
        if failures == 0:
            print(
                "OK: no legacy friends left, every sharded friend sits in its derived shard."
            )
        return 1 if failures else 0
    finally:
        await pool.close()


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Backfill legacy m.friends.list entries into W5 shards"
    )
    parser.add_argument(
        "--database-url",
        default=DEFAULT_DATABASE_URL,
        help="PostgreSQL DSN (default: $DATABASE_URL)",
    )
    parser.add_argument(
        "--dry-run", action="store_true", help="print the plan without writing"
    )
    parser.add_argument(
        "--verify",
        action="store_true",
        help="check the post-backfill invariants and exit",
    )
    parser.add_argument(
        "--limit", type=int, default=None, help="process at most N rooms (debugging)"
    )
    args = parser.parse_args()

    if not args.database_url:
        print("error: --database-url or DATABASE_URL is required", file=sys.stderr)
        return 2

    if args.verify:
        return asyncio.run(verify(args.database_url))
    return asyncio.run(run(args.database_url, args.dry_run, args.limit))


if __name__ == "__main__":
    sys.exit(main())
