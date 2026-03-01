#!/bin/bash
# ============================================================================
# Database Schema Verification Script
# Purpose: Verify all required tables and columns exist after migration
# ============================================================================

set -e

echo "=========================================="
echo "Database Schema Verification Script"
echo "=========================================="

# Configuration
DB_HOST="${DB_HOST:-db}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-synapse_test}"
DB_USER="${DB_USER:-synapse}"
DB_PASSWORD="${DB_PASSWORD:-synapse}"

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

ERRORS=0
WARNINGS=0

# Function to check if table exists
check_table() {
    local table_name=$1
    local result=$(PGPASSWORD=$DB_PASSWORD psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -c \
        "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_schema = 'public' AND table_name = '$table_name');" | tr -d ' ')
    
    if [ "$result" = "t" ]; then
        echo -e "${GREEN}[PASS]${NC} Table '$table_name' exists"
        return 0
    else
        echo -e "${RED}[FAIL]${NC} Table '$table_name' does not exist"
        ERRORS=$((ERRORS + 1))
        return 1
    fi
}

# Function to check if column exists in table
check_column() {
    local table_name=$1
    local column_name=$2
    local result=$(PGPASSWORD=$DB_PASSWORD psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -c \
        "SELECT EXISTS (SELECT FROM information_schema.columns WHERE table_schema = 'public' AND table_name = '$table_name' AND column_name = '$column_name');" | tr -d ' ')
    
    if [ "$result" = "t" ]; then
        echo -e "${GREEN}[PASS]${NC} Column '$column_name' in table '$table_name' exists"
        return 0
    else
        echo -e "${RED}[FAIL]${NC} Column '$column_name' in table '$table_name' does not exist"
        ERRORS=$((ERRORS + 1))
        return 1
    fi
}

# Function to check if index exists
check_index() {
    local index_name=$1
    local result=$(PGPASSWORD=$DB_PASSWORD psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -c \
        "SELECT EXISTS (SELECT FROM pg_indexes WHERE indexname = '$index_name');" | tr -d ' ')
    
    if [ "$result" = "t" ]; then
        echo -e "${GREEN}[PASS]${NC} Index '$index_name' exists"
        return 0
    else
        echo -e "${YELLOW}[WARN]${NC} Index '$index_name' does not exist"
        WARNINGS=$((WARNINGS + 1))
        return 1
    fi
}

echo ""
echo "=== Checking Voice Message Tables ==="
check_table "voice_usage_stats"
check_column "voice_usage_stats" "user_id"
check_column "voice_usage_stats" "room_id"
check_column "voice_usage_stats" "total_duration_ms"
check_column "voice_usage_stats" "message_count"

echo ""
echo "=== Checking Space Tables ==="
check_table "spaces"
check_column "spaces" "space_id"
check_column "spaces" "room_id"
check_column "spaces" "creator"
check_column "spaces" "is_public"

check_table "space_members"
check_column "space_members" "space_id"
check_column "space_members" "user_id"
check_column "space_members" "membership"
check_column "space_members" "joined_ts"
check_column "space_members" "left_ts"
check_column "space_members" "updated_ts"

check_table "space_children"
check_column "space_children" "space_id"
check_column "space_children" "room_id"
check_column "space_children" "suggested"
check_column "space_children" "via_servers"

check_table "space_summaries"
check_column "space_summaries" "space_id"
check_column "space_summaries" "summary"
check_column "space_summaries" "children_count"
check_column "space_summaries" "member_count"

check_table "space_events"
check_column "space_events" "event_id"
check_column "space_events" "space_id"
check_column "space_events" "event_type"
check_column "space_events" "sender"
check_column "space_events" "content"

echo ""
echo "=== Checking Room Hierarchy Tables ==="
check_table "room_parents"

echo ""
echo "=== Checking Core Tables ==="
check_table "users"
check_table "rooms"
check_table "events"
check_table "room_members"

echo ""
echo "=== Checking Indexes ==="
check_index "idx_voice_usage_stats_user"
check_index "idx_spaces_room"
check_index "idx_space_members_space"
check_index "space_children_unique"

echo ""
echo "=========================================="
echo "Verification Summary"
echo "=========================================="
echo -e "Errors: ${RED}$ERRORS${NC}"
echo -e "Warnings: ${YELLOW}$WARNINGS${NC}"

if [ $ERRORS -eq 0 ]; then
    echo -e "${GREEN}All required tables and columns exist!${NC}"
    exit 0
else
    echo -e "${RED}Some required tables or columns are missing!${NC}"
    exit 1
fi
