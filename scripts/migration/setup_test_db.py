#!/usr/bin/env python3
"""
A3+A4 State Group Test Data Generator

Creates test databases, migrations, and sample data for testing
per-event state group implementation.

Usage:
    python3 setup_test_db.py --help
    python3 setup_test_db.py --setup          # Full setup (drop + recreate)
    python3 setup_test_db.py --migrate       # Apply migrations only
    python3 setup_test_db.py --seed          # Seed sample data only
    python3 setup_test_db.py --verify        # Verify test environment
"""

import argparse
import asyncio
import hashlib
import json
import sys
from datetime import datetime, timezone
from typing import Optional

import asyncpg
from dotenv import load_dotenv

load_dotenv()

# Database configuration
DB_HOST = "localhost"
DB_PORT = 15432  # Docker exposed port
TEST_DB = "synapse_test"
TEST_TEMPLATE = "test_template_ci"
DB_USER = "synapse"
DB_PASSWORD = "synapse"

BASE_URL = f"postgresql://{DB_USER}:{DB_PASSWORD}@{DB_HOST}:{DB_PORT}/postgres"
TEST_URL = f"postgresql://{DB_USER}:{DB_PASSWORD}@{DB_HOST}:{DB_PORT}/{TEST_DB}"


def hash_sha256(data: str) -> str:
    """Generate SHA-256 hash (same as synapse's content_hash)."""
    return hashlib.sha256(data.encode()).hexdigest()


def base64_url_safe(hash_bytes: bytes) -> str:
    """Convert hash to URL-safe base64."""
    import base64

    return base64.urlsafe_b64encode(hash_bytes).rstrip(b"=").decode()


async def drop_databases(pool: asyncpg.Pool):
    """Drop test databases if they exist."""
    print("🗑️  Dropping existing test databases...")

    async with pool.acquire() as conn:
        # Terminate connections
        await conn.execute(
            """
            SELECT pg_terminate_backend(pid)
            FROM pg_stat_activity
            WHERE datname IN ($1, $2) AND pid <> pg_backend_pid()
        """,
            TEST_DB,
            TEST_TEMPLATE,
        )

        # Drop databases
        for dbname in [TEST_DB, TEST_TEMPLATE]:
            try:
                await conn.execute(f"DROP DATABASE IF EXISTS {dbname}")
                print(f"  ✓ Dropped {dbname}")
            except Exception as e:
                print(f"  ⚠️  Warning dropping {dbname}: {e}")


async def create_test_database(pool: asyncpg.Pool):
    """Create the main test database."""
    print("\n📦 Creating test database...")

    async with pool.acquire() as conn:
        await conn.execute(f"CREATE DATABASE {TEST_DB}")
        print(f"  ✓ Created database {TEST_DB}")


async def create_test_template(pool: asyncpg.Pool):
    """Create the CI test template database."""
    print("\n📦 Creating test template database...")

    async with pool.acquire() as conn:
        # Create template database
        await conn.execute(f"CREATE DATABASE {TEST_TEMPLATE}")
        print(f"  ✓ Created template database {TEST_TEMPLATE}")


async def apply_migrations(pool: asyncpg.Pool, db_name: str):
    """Apply the unified schema migration."""
    print(f"\n🔧 Applying migrations to {db_name}...")

    # Read the migration file
    migration_path = "migrations/00000000_unified_schema_v12.sql"
    try:
        with open(migration_path, "r") as f:
            migration_sql = f.read()
        print(f"  ✓ Loaded migration from {migration_path}")
    except FileNotFoundError:
        print(f"  ❌ Migration file not found: {migration_path}")
        return False

    # Connect to specific database and execute
    db_url = f"postgresql://{DB_USER}:{DB_PASSWORD}@{DB_HOST}:{DB_PORT}/{db_name}"
    migration_conn = await asyncpg.connect(db_url)

    try:
        await migration_conn.execute(migration_sql)
        print(f"  ✓ Applied unified schema v12 to {db_name}")
        return True
    except Exception as e:
        print(f"  ❌ Migration failed: {e}")
        return False
    finally:
        await migration_conn.close()


async def seed_sample_data(pool: asyncpg.Pool):
    """Seed sample data for A3/A4 testing."""
    print("\n🌱 Seeding sample data...")

    async with pool.acquire() as conn:
        # 1. Create test users
        print("  👥 Creating test users...")
        users = [
            ("@alice:test.local", "Alice", "alice_avatar"),
            ("@bob:test.local", "Bob", "bob_avatar"),
            ("@charlie:test.local", "Charlie", None),
        ]

        for user_id, display_name, avatar in users:
            await conn.execute(
                """
                INSERT INTO profiles (user_id, display_name, avatar_url)
                VALUES ($1, $2, $3)
                ON CONFLICT (user_id) DO NOTHING
            """,
                user_id,
                display_name,
                avatar,
            )

        print(f"    ✓ Created {len(users)} users")

        # 2. Create test rooms (v12 - domainless)
        print("  🏠 Creating test rooms...")
        rooms = []
        room_versions = [12, 12, 12]  # All v12 for A3/A4 testing

        for i, version in enumerate(room_versions):
            room_id = f"!room{i + 1}:test.local"
            creator = "@alice:test.local"

            # Insert room
            await conn.execute(
                """
                INSERT INTO rooms (room_id, creator, room_version, created_ts)
                VALUES ($1, $2, $3, $4)
            """,
                room_id,
                creator,
                version,
                int(datetime.now(timezone.utc).timestamp() * 1000) - (i * 3600000),
            )

            rooms.append((room_id, creator))
            print(f"    ✓ Created room {room_id} (v{version})")

        # 3. Create events and state groups
        print("  📝 Creating events with state groups...")

        for idx, (room_id, creator) in enumerate(rooms):
            # Create event (MSC4291 format - no room_id in PDU)
            create_event_id = f"$create:{room_id[1:]}"
            create_ts = int(datetime.now(timezone.utc).timestamp() * 1000) - (
                (idx + 1) * 7200000
            )

            # Insert create event
            create_content = {"creator": creator, "m.federate": True}
            await conn.execute(
                """
                INSERT INTO events (
                    event_id, room_id, sender, event_type, state_key,
                    content, origin_server_ts, depth, is_redacted
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false)
            """,
                create_event_id,
                room_id,
                creator,
                "m.room.create",
                "",
                json.dumps(create_content),
                create_ts,
                1,
            )

            # Create membership state event
            member_event_id = f"$member:{creator[1:]}"
            member_content = {"membership": "join", "displayname": "Alice"}

            await conn.execute(
                """
                INSERT INTO events (
                    event_id, room_id, sender, event_type, state_key,
                    content, origin_server_ts, depth, is_redacted
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false)
            """,
                member_event_id,
                room_id,
                creator,
                "m.room.member",
                creator,
                json.dumps(member_content),
                create_ts + 1000,
                2,
            )

            # Create power_levels event
            pl_event_id = f"$powerlevels:{room_id[1:]}"
            pl_content = {
                "users": {creator: 100},
                "events": {"m.room.name": 50, "m.room.power_levels": 100},
                "state_default": 50,
                "events_default": 0,
            }

            await conn.execute(
                """
                INSERT INTO events (
                    event_id, room_id, sender, event_type, state_key,
                    content, origin_server_ts, depth, is_redacted
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false)
            """,
                pl_event_id,
                room_id,
                creator,
                "m.room.power_levels",
                "",
                json.dumps(pl_content),
                create_ts + 2000,
                3,
            )

            # NOW: Create initial state group for this room (A3+A4 backfill simulation)
            print(f"    🔄 Creating state group for {room_id}...")

            # Compute state hash
            state_entries = [
                ("m.room.create", "", create_event_id),
                ("m.room.member", creator, member_event_id),
                ("m.room.power_levels", "", pl_event_id),
            ]
            state_hash_input = json.dumps(state_entries, sort_keys=True)
            state_hash = base64_url_safe(hash_sha256(state_hash_input).encode()).lower()

            # Create state group
            group_id = await conn.fetchval(
                """
                INSERT INTO state_groups (room_id, event_id, state_hash, created_ts)
                VALUES ($1, $2, $3, $4)
                RETURNING id
            """,
                room_id,
                member_event_id,
                state_hash,
                create_ts + 2000,
            )

            print(f"      ✓ Created state_group {group_id}")

            # Add state entries
            await conn.executemany(
                """
                INSERT INTO state_group_state (state_group_id, event_type, state_key, event_id)
                VALUES ($1, $2, $3, $4)
            """,
                [(group_id, etype, skey, eid) for etype, skey, eid in state_entries],
            )

            print(f"      ✓ Added {len(state_entries)} state entries")

            # Bind events to state group
            await conn.executemany(
                """
                INSERT INTO event_to_state_groups (event_id, state_group_id)
                VALUES ($1, $2)
            """,
                [(eid, group_id) for _, _, eid in state_entries],
            )

            print(f"      ✓ Bound {len(state_entries)} events to state group")

            # Add some regular messages (these would be bound in A3 implementation)
            for msg_idx in range(3):
                msg_event_id = f"$message{msg_idx}:{room_id[1:]}"
                msg_content = {"body": f"Message {msg_idx + 1}", "msgtype": "m.text"}

                await conn.execute(
                    """
                    INSERT INTO events (
                        event_id, room_id, sender, event_type, state_key,
                        content, origin_server_ts, depth, is_redacted
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false)
                """,
                    msg_event_id,
                    room_id,
                    creator,
                    "m.room.message",
                    None,
                    json.dumps(msg_content),
                    create_ts + 3000 + (msg_idx * 1000),
                    4 + msg_idx,
                )

                # In A3 implementation, these would be bound to the state group
                # For now, we'll bind them to simulate completed A3
                await conn.execute(
                    """
                    INSERT INTO event_to_state_groups (event_id, state_group_id)
                    VALUES ($1, $2)
                """,
                    msg_event_id,
                    group_id,
                )

            print(f"      ✓ Created and bound 3 messages")

        # 4. Create a forked room scenario (for testing A3 fork resolution)
        print("  🔀 Creating forked room scenario...")

        fork_room_id = "!room_fork:test.local"
        await conn.execute(
            """
            INSERT INTO rooms (room_id, creator, room_version, created_ts)
            VALUES ($1, $2, $3, $4)
        """,
            fork_room_id,
            "@alice:test.local",
            12,
            int(datetime.now(timezone.utc).timestamp() * 1000) - 86400000,
        )

        # Create events for both users (simulating concurrent sends)
        alice_msg_id = "$alice_concurrent:test.local"
        bob_msg_id = "$bob_concurrent:test.local"

        for msg_id, sender in [
            (alice_msg_id, "@alice:test.local"),
            (bob_msg_id, "@bob:test.local"),
        ]:
            await conn.execute(
                """
                INSERT INTO events (
                    event_id, room_id, sender, event_type, content,
                    origin_server_ts, depth, prev_events, is_redacted
                )
                VALUES ($1, $2, $3, 'm.room.message', $4, $5, $6, $7, false)
            """,
                msg_id,
                fork_room_id,
                sender,
                json.dumps(
                    {"body": f"Concurrent message by {sender}", "msgtype": "m.text"}
                ),
                int(datetime.now(timezone.utc).timestamp() * 1000),
                10,
                "[]",
            )

        # Simulate forward extremities (fork)
        await conn.execute(
            """
            INSERT INTO event_extremities (event_id, room_id, depth)
            VALUES ($1, $2, 10), ($3, $4, 10)
        """,
            alice_msg_id,
            fork_room_id,
            bob_msg_id,
            fork_room_id,
        )

        print("    ✓ Forked room created (ready for A3 fork resolution testing)")

        # 5. Create unfilled room (for A4 backfill testing)
        print("  📊 Creating unfilled room (A4 backfill test)...")

        unfilled_room_id = "!room_unfilled:test.local"
        await conn.execute(
            """
            INSERT INTO rooms (room_id, creator, room_version, created_ts)
            VALUES ($1, $2, $3, $4)
        """,
            unfilled_room_id,
            "@alice:test.local",
            12,
            int(datetime.now(timezone.utc).timestamp() * 1000) - 172800000,
        )

        # Add events WITHOUT state groups (this simulates pre-A3 state)
        for i in range(3):
            event_id = f"$unfilled_msg{i}:{unfilled_room_id[1:]}"
            await conn.execute(
                """
                INSERT INTO events (
                    event_id, room_id, sender, event_type, state_key,
                    content, origin_server_ts, depth, is_redacted
                )
                VALUES ($1, $2, $3, 'm.room.message', NULL, $4, $5, $6, false)
            """,
                event_id,
                unfilled_room_id,
                "@alice:test.local",
                json.dumps({"body": f"Unfilled message {i}", "msgtype": "m.text"}),
                int(datetime.now(timezone.utc).timestamp() * 1000) - (i * 1000),
                5 + i,
            )

        print("    ✓ Unfilled room created (has events but no state_groups)")

        # 6. Verify summary
        total_rooms = await conn.fetchval("SELECT COUNT(*) FROM rooms")
        total_events = await conn.fetchval("SELECT COUNT(*) FROM events")
        total_state_groups = await conn.fetchval("SELECT COUNT(*) FROM state_groups")
        unfilled_rooms = await conn.fetchval("""
            SELECT COUNT(*) FROM rooms r
            WHERE r.room_version >= 12
              AND NOT EXISTS (SELECT 1 FROM state_groups sg WHERE sg.room_id = r.room_id)
        """)

        print("\n📊 Summary:")
        print(f"    • Total rooms: {total_rooms}")
        print(f"    • Total events: {total_events}")
        print(f"    • Rooms with state groups: {total_state_groups}")
        print(f"    • Unfilled v12 rooms: {unfilled_rooms} (for A4 backfill testing)")


async def verify_test_environment(pool: asyncpg.Pool):
    """Verify test environment is properly set up."""
    print("\n🔍 Verifying test environment...")

    async with pool.acquire() as conn:
        # Check database exists
        db_exists = await conn.fetchval(
            """
            SELECT COUNT(*) FROM pg_database WHERE datname = $1
        """,
            TEST_DB,
        )
        print(f"  {'✓' if db_exists else '✗'} Test database exists")

        # Check tables exist
        tables = [
            "rooms",
            "events",
            "state_groups",
            "state_group_state",
            "event_to_state_groups",
        ]
        for table in tables:
            exists = await conn.fetchval(
                """
                SELECT COUNT(*) FROM information_schema.tables 
                WHERE table_schema = 'public' AND table_name = $1
            """,
                table,
            )
            print(f"  {'✓' if exists else '✗'} Table {table} exists")

        # Check sample data
        total_rooms = await conn.fetchval("SELECT COUNT(*) FROM rooms")
        total_state_groups = await conn.fetchval("SELECT COUNT(*) FROM state_groups")

        print(
            f"  ℹ️  Sample data: {total_rooms} rooms, {total_state_groups} state groups"
        )

        # Check for unfilled rooms
        unfilled = await conn.fetchval("""
            SELECT COUNT(*) FROM rooms r
            WHERE r.room_version >= 12
              AND NOT EXISTS (SELECT 1 FROM state_groups sg WHERE sg.room_id = r.room_id)
        """)

        if unfilled > 0:
            print(
                f"  ✅ Found {unfilled} unfilled rooms (ready for A4 backfill testing)"
            )
        else:
            print(f"  ⚠️  No unfilled rooms found (all v12 rooms have state groups)")

        # Check migration fingerprint
        fingerprint = await conn.fetchval("""
            SELECT checksum FROM room_versions WHERE version = '12'
        """)
        if fingerprint:
            print(f"  ✓ Room v12 fingerprint: {fingerprint}")
        else:
            print(f"  ⚠️  Could not verify room v12 fingerprint")

        print("\n✅ Test environment verification complete")


async def main():
    parser = argparse.ArgumentParser(description="A3+A4 Test Database Setup")
    parser.add_argument(
        "--setup",
        action="store_true",
        help="Full setup (drop + create + migrate + seed)",
    )
    parser.add_argument("--migrate", action="store_true", help="Apply migrations only")
    parser.add_argument("--seed", action="store_true", help="Seed sample data only")
    parser.add_argument("--verify", action="store_true", help="Verify test environment")
    args = parser.parse_args()

    if not any([args.setup, args.migrate, args.seed, args.verify]):
        parser.print_help()
        return 1

    try:
        # Connect to postgres database
        postgres_pool = await asyncpg.create_pool(BASE_URL, min_size=1)

        if args.setup:
            await drop_databases(postgres_pool)
            await create_test_database(postgres_pool)
            await create_test_template(postgres_pool)
            await apply_migrations(postgres_pool, TEST_DB)
            await apply_migrations(postgres_pool, TEST_TEMPLATE)
            await seed_sample_data(postgres_pool)

        if args.migrate:
            await apply_migrations(postgres_pool, TEST_DB)
            await apply_migrations(postgres_pool, TEST_TEMPLATE)

        if args.seed:
            await seed_sample_data(postgres_pool)

        if args.verify:
            await verify_test_environment(postgres_pool)

        await postgres_pool.close()
        return 0

    except Exception as e:
        print(f"\n❌ Error: {e}")
        import traceback

        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
