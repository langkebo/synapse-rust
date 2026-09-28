#!/usr/bin/env python3
"""
A4: Backfill state groups for existing rooms (v12+)

This script finds all v12+ rooms without state group records,
computes their current state using timestamp derivation (one-time),
and creates initial state groups for them.

Usage:
    python3 scripts/migration/backfill_state_groups.py --dry-run  # preview
    python3 scripts/migration/backfill_state_groups.py          # execute
    python3 scripts/migration/backfill_state_groups.py --verify  # verify completion

Example output:
    Found 3 v12+ rooms without state groups
    Backfilling room !room1:test.local...
    ✓ Created state_group for !room1:test.local
    Backfilling room !room2:test.local...
    ✓ Created state_group for !room2:test.local
    
    Summary: Backfilled 2/3 rooms (1 skipped - already has state group)
"""

import argparse
import asyncio
import sys
from datetime import datetime, timezone
from typing import Optional

import asyncpg
from dotenv import load_dotenv

load_dotenv()

# Database configuration
DB_HOST = "localhost"
DB_PORT = 15432
TEST_DB = "synapse_test"
DB_USER = "synapse"
DB_PASSWORD = "synapse"

TEST_URL = f"postgresql://{DB_USER}:{DB_PASSWORD}@{DB_HOST}:{DB_PORT}/{TEST_DB}"


async def find_unbackfilled_rooms(pool: asyncpg.Pool) -> list:
    """Find v12+ rooms without state_group records."""
    query = """
        SELECT r.room_id, r.room_version, r.created_ts
        FROM rooms r
        WHERE r.room_version >= 12
          AND NOT EXISTS (
              SELECT 1 FROM state_groups sg WHERE sg.room_id = r.room_id
          )
        ORDER BY r.created_ts ASC
    """
    return await pool.fetch(query)


async def compute_current_state_via_timestamp(pool: asyncpg.Pool, room_id: str) -> list:
    """
    Compute current state using timestamp derivation (one-time fallback).
    
    This is the exact same logic as get_state_events() when no state group exists.
    """
    query = """
        SELECT DISTINCT ON (event_type, state_key)
               event_id, event_type, state_key, content, origin_server_ts
        FROM events
        WHERE room_id = $1 AND state_key IS NOT NULL
        ORDER BY event_type, state_key, origin_server_ts DESC, event_id DESC
    """
    return await pool.fetch(query, room_id)


async def create_initial_state_group(
    pool: asyncpg.Pool,
    room_id: str,
    state_events: list,
    last_event_id: Optional[str] = None
) -> Optional[int]:
    """
    Create initial state group and bind state events.
    
    Returns the new group_id, or None if no state events found.
    """
    if not state_events:
        return None
    
    # Use the most recent event as the anchor
    if last_event_id is None:
        last_event_id = state_events[0]['event_id']
    
    # Compute state hash (simplified - just hash the state entries)
    state_hash_input = str([(e['event_type'], e['state_key'], e['event_id']) 
                           for e in state_events]).encode()
    state_hash = state_hash_input.hex()[:64]  # Simple hash for demo
    
    # Create state group in a transaction
    async with pool.acquire() as conn:
        async with conn.transaction():
            # Insert state_group
            group_id = await conn.fetchval("""
                INSERT INTO state_groups (room_id, event_id, state_hash, created_ts)
                VALUES ($1, $2, $3, $4)
                RETURNING id
            """, room_id, last_event_id, state_hash, int(datetime.now(timezone.utc).timestamp() * 1000))
            
            # Insert state_group_state and bind events
            for event in state_events:
                await conn.execute("""
                    INSERT INTO state_group_state (state_group_id, event_type, state_key, event_id)
                    VALUES ($1, $2, $3, $4)
                    ON CONFLICT (state_group_id, event_type, state_key) DO NOTHING
                """, group_id, event['event_type'], event['state_key'], event['event_id'])
                
                # Bind event to state group
                await conn.execute("""
                    INSERT INTO event_to_state_groups (event_id, state_group_id)
                    VALUES ($1, $2)
                    ON CONFLICT (event_id) DO UPDATE SET state_group_id = $2
                """, event['event_id'], group_id)
            
            return group_id


async def backfill_room(pool: asyncpg.Pool, room_id: str) -> dict:
    """
    Backfill a single room with state groups.
    
    Returns a dict with the result status.
    """
    # Compute current state using timestamp derivation
    state_events = await compute_current_state_via_timestamp(pool, room_id)
    
    if not state_events:
        return {"room_id": room_id, "status": "no_state_events"}
    
    # Create initial state group
    group_id = await create_initial_state_group(pool, room_id, state_events)
    
    if group_id is None:
        return {"room_id": room_id, "status": "failed_to_create_group"}
    
    return {
        "room_id": room_id,
        "status": "success",
        "group_id": group_id,
        "state_events_count": len(state_events)
    }


async def verify_backfill(pool: asyncpg.Pool) -> dict:
    """Verify that all v12+ rooms now have state groups."""
    # Count total v12+ rooms
    total = await pool.fetchval("""
        SELECT COUNT(*) FROM rooms WHERE room_version >= 12
    """)
    
    # Count v12+ rooms with state groups
    with_groups = await pool.fetchval("""
        SELECT COUNT(*) FROM rooms r
        WHERE r.room_version >= 12
          AND EXISTS (SELECT 1 FROM state_groups sg WHERE sg.room_id = r.room_id)
    """)
    
    # Count v12+ rooms without state groups (should be 0)
    without_groups = await pool.fetchval("""
        SELECT COUNT(*) FROM rooms r
        WHERE r.room_version >= 12
          AND NOT EXISTS (SELECT 1 FROM state_groups sg WHERE sg.room_id = r.room_id)
    """)
    
    return {
        "total_v12_rooms": total,
        "rooms_with_state_groups": with_groups,
        "rooms_without_state_groups": without_groups,
        "backfill_complete": without_groups == 0
    }


async def main():
    parser = argparse.ArgumentParser(description="A4: Backfill state groups for v12+ rooms")
    parser.add_argument("--dry-run", action="store_true", help="Preview without making changes")
    parser.add_argument("--verify", action="store_true", help="Verify backfill completion")
    args = parser.parse_args()
    
    try:
        pool = await asyncpg.create_pool(TEST_URL, min_size=1)
        
        if args.verify:
            result = await verify_backfill(pool)
            print("\n📊 Backfill Verification Report:")
            print(f"   Total v12+ rooms: {result['total_v12_rooms']}")
            print(f"   Rooms with state groups: {result['rooms_with_state_groups']}")
            print(f"   Rooms without state groups: {result['rooms_without_state_groups']}")
            
            if result['backfill_complete']:
                print("\n✅ All v12+ rooms have state groups!")
                return 0
            else:
                print(f"\n⚠️  {result['rooms_without_state_groups']} rooms still need backfill")
                return 1
        
        # Find rooms that need backfill
        rooms = await find_unbackfilled_rooms(pool)
        print(f"\n📊 Found {len(rooms)} v12+ rooms without state groups")
        
        if not rooms:
            print("✅ All v12+ rooms already have state groups!")
            return 0
        
        # Preview
        if args.dry_run:
            print("\n🔍 DRY RUN - Preview:")
            for room in rooms:
                print(f"   Would backfill: {room['room_id']} (v{room['room_version']})")
            print("\n✅ Dry run complete. Use --execute to apply changes.")
            return 0
        
        # Execute backfill
        print("\n🚀 Starting backfill...")
        success_count = 0
        fail_count = 0
        
        for room in rooms:
            room_id = room['room_id']
            print(f"   Backfilling {room_id}...", end=" ")
            
            try:
                result = await backfill_room(pool, room_id)
                if result['status'] == 'success':
                    print(f"✓ (group_id={result['group_id']}, {result['state_events_count']} events)")
                    success_count += 1
                else:
                    print(f"⚠️  {result['status']}")
                    fail_count += 1
            except Exception as e:
                print(f"❌ Error: {e}")
                fail_count += 1
        
        print(f"\n📈 Summary: {success_count} succeeded, {fail_count} failed")
        
        # Final verification
        verification = await verify_backfill(pool)
        if verification['backfill_complete']:
            print("\n✅ All v12+ rooms now have state groups!")
            print("   Ready to remove timestamp derivation fallback (A4-ii)")
        else:
            print(f"\n⚠️  {verification['rooms_without_state_groups']} rooms still missing state groups")
        
        await pool.close()
        return 0
    
    except Exception as e:
        print(f"\n❌ Fatal error: {e}")
        import traceback
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
