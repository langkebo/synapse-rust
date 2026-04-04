#!/bin/bash
# ============================================================================
# 基准数据生成工具
# 创建日期: 2026-04-04
# 描述: 为性能基准测试生成可重复的测试数据
# ============================================================================

set -e

# 配置
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
DOCKER_COMPOSE_FILE="$PROJECT_ROOT/docker/docker-compose.yml"
DB_SERVICE_NAME="db"
DB_NAME="synapse_test"
DB_USER="synapse"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# 日志函数
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

compose_exec_db() {
    docker compose -f "$DOCKER_COMPOSE_FILE" exec -T "$DB_SERVICE_NAME" "$@"
}

# 检查 Docker 容器是否运行
check_container() {
    if ! docker compose -f "$DOCKER_COMPOSE_FILE" ps "$DB_SERVICE_NAME" | grep -q "running"; then
        log_error "Service $DB_SERVICE_NAME is not running"
        log_info "Starting container..."
        docker compose -f "$DOCKER_COMPOSE_FILE" up -d "$DB_SERVICE_NAME"
        sleep 5
    fi
    log_success "Service $DB_SERVICE_NAME is running"
}

# 生成用户数据
generate_users() {
    local count=$1
    local seed=${2:-42}

    log_info "Generating $count users (seed: $seed)..."

    compose_exec_db psql -U "$DB_USER" -d "$DB_NAME" <<EOF
DO \$\$
DECLARE
    i INTEGER;
    user_id TEXT;
    username TEXT;
    created_ts BIGINT;
BEGIN
    -- 设置随机种子以保证可重复性
    PERFORM setseed(${seed} / 2147483647.0);

    FOR i IN 1..$count LOOP
        username := 'bench_user_' || i;
        user_id := '@' || username || ':benchmark.local';
        created_ts := extract(epoch from now())::BIGINT * 1000 - (random() * 86400000 * 365)::BIGINT;

        INSERT INTO users (user_id, username, password_hash, is_admin, created_ts, is_guest)
        VALUES (
            user_id,
            username,
            '\$argon2id\$v=19\$m=65536,t=3,p=1\$benchmark\$hash',
            false,
            created_ts,
            false
        )
        ON CONFLICT (user_id) DO NOTHING;

        IF i % 1000 = 0 THEN
            RAISE NOTICE 'Generated % users', i;
        END IF;
    END LOOP;
END \$\$;
EOF

    log_success "Generated $count users"
}

# 生成房间数据
generate_rooms() {
    local count=$1
    local seed=${2:-42}

    log_info "Generating $count rooms (seed: $seed)..."

    compose_exec_db psql -U "$DB_USER" -d "$DB_NAME" <<EOF
DO \$\$
DECLARE
    i INTEGER;
    room_id TEXT;
    creator TEXT;
    created_ts BIGINT;
BEGIN
    PERFORM setseed(${seed} / 2147483647.0);

    FOR i IN 1..$count LOOP
        room_id := '!bench_room_' || i || ':benchmark.local';
        creator := '@bench_user_' || (1 + floor(random() * 100)::INTEGER) || ':benchmark.local';
        created_ts := extract(epoch from now())::BIGINT * 1000 - (random() * 86400000 * 180)::BIGINT;

        INSERT INTO rooms (room_id, creator, created_ts, is_public, room_version)
        VALUES (
            room_id,
            creator,
            created_ts,
            random() > 0.5,
            '10'
        )
        ON CONFLICT (room_id) DO NOTHING;

        IF i % 100 = 0 THEN
            RAISE NOTICE 'Generated % rooms', i;
        END IF;
    END LOOP;
END \$\$;
EOF

    log_success "Generated $count rooms"
}

# 生成事件数据
generate_events() {
    local count=$1
    local seed=${2:-42}

    log_info "Generating $count events (seed: $seed)..."

    compose_exec_db psql -U "$DB_USER" -d "$DB_NAME" <<EOF
DO \$\$
DECLARE
    i INTEGER;
    event_id TEXT;
    room_id TEXT;
    sender TEXT;
    event_type TEXT;
    origin_server_ts BIGINT;
    content JSONB;
    room_count INTEGER;
BEGIN
    PERFORM setseed(${seed} / 2147483647.0);

    SELECT COUNT(*) INTO room_count FROM rooms WHERE room_id LIKE '!bench_room_%';
    IF room_count = 0 THEN
        RAISE EXCEPTION 'No benchmark rooms found. Generate rooms first.';
    END IF;

    FOR i IN 1..$count LOOP
        event_id := '\$bench_event_' || i || ':benchmark.local';
        room_id := '!bench_room_' || (1 + floor(random() * LEAST(room_count, 100))::INTEGER) || ':benchmark.local';
        sender := '@bench_user_' || (1 + floor(random() * 100)::INTEGER) || ':benchmark.local';
        event_type := 'm.room.message';
        origin_server_ts := extract(epoch from now())::BIGINT * 1000 - (random() * 86400000 * 30)::BIGINT;
        content := jsonb_build_object(
            'msgtype', 'm.text',
            'body', 'Benchmark message ' || i
        );

        INSERT INTO events (
            event_id, room_id, sender, event_type,
            origin_server_ts, content, state_key
        )
        VALUES (
            event_id, room_id, sender, event_type,
            origin_server_ts, content, NULL
        )
        ON CONFLICT (event_id) DO NOTHING;

        IF i % 1000 = 0 THEN
            RAISE NOTICE 'Generated % events', i;
        END IF;
    END LOOP;
END \$\$;
EOF

    log_success "Generated $count events"
}

# 生成设备数据
generate_devices() {
    local count=$1
    local seed=${2:-42}

    log_info "Generating $count devices (seed: $seed)..."

    compose_exec_db psql -U "$DB_USER" -d "$DB_NAME" <<EOF
DO \$\$
DECLARE
    i INTEGER;
    device_id TEXT;
    user_id TEXT;
    display_name TEXT;
    last_seen_ts BIGINT;
BEGIN
    PERFORM setseed(${seed} / 2147483647.0);

    FOR i IN 1..$count LOOP
        device_id := 'BENCH_DEVICE_' || i;
        user_id := '@bench_user_' || (1 + floor(random() * 100)::INTEGER) || ':benchmark.local';
        display_name := 'Benchmark Device ' || i;
        last_seen_ts := extract(epoch from now())::BIGINT * 1000 - (random() * 86400000 * 7)::BIGINT;

        INSERT INTO devices (device_id, user_id, display_name, last_seen_ts)
        VALUES (device_id, user_id, display_name, last_seen_ts)
        ON CONFLICT (device_id, user_id) DO NOTHING;

        IF i % 1000 = 0 THEN
            RAISE NOTICE 'Generated % devices', i;
        END IF;
    END LOOP;
END \$\$;
EOF

    log_success "Generated $count devices"
}

# 清理基准数据
cleanup_benchmark_data() {
    log_warning "Cleaning up benchmark data..."

    compose_exec_db psql -U "$DB_USER" -d "$DB_NAME" <<EOF
DELETE FROM events WHERE event_id LIKE '\$bench_event_%';
DELETE FROM devices WHERE device_id LIKE 'BENCH_DEVICE_%';
DELETE FROM rooms WHERE room_id LIKE '!bench_room_%';
DELETE FROM users WHERE user_id LIKE '@bench_user_%';
EOF

    log_success "Benchmark data cleaned up"
}

# 显示数据统计
show_stats() {
    log_info "Benchmark data statistics:"

    compose_exec_db psql -U "$DB_USER" -d "$DB_NAME" <<EOF
SELECT
    'Users' as type,
    COUNT(*) as count
FROM users WHERE user_id LIKE '@bench_user_%'
UNION ALL
SELECT
    'Rooms' as type,
    COUNT(*) as count
FROM rooms WHERE room_id LIKE '!bench_room_%'
UNION ALL
SELECT
    'Events' as type,
    COUNT(*) as count
FROM events WHERE event_id LIKE '\$bench_event_%'
UNION ALL
SELECT
    'Devices' as type,
    COUNT(*) as count
FROM devices WHERE device_id LIKE 'BENCH_DEVICE_%';
EOF
}

# 预设数据集
generate_preset() {
    local preset=$1

    case "$preset" in
        small)
            log_info "Generating SMALL dataset (1K users, 100 rooms, 10K events)"
            generate_users 1000
            generate_rooms 100
            generate_events 10000
            generate_devices 2000
            ;;
        medium)
            log_info "Generating MEDIUM dataset (10K users, 1K rooms, 100K events)"
            generate_users 10000
            generate_rooms 1000
            generate_events 100000
            generate_devices 20000
            ;;
        large)
            log_info "Generating LARGE dataset (100K users, 10K rooms, 1M events)"
            generate_users 100000
            generate_rooms 10000
            generate_events 1000000
            generate_devices 200000
            ;;
        *)
            log_error "Unknown preset: $preset"
            echo "Available presets: small, medium, large"
            exit 1
            ;;
    esac

    show_stats
}

# 主函数
main() {
    local command="${1:-help}"

    case "$command" in
        users)
            check_container
            local count="${2:-1000}"
            local seed="${3:-42}"
            generate_users "$count" "$seed"
            ;;
        rooms)
            check_container
            local count="${2:-100}"
            local seed="${3:-42}"
            generate_rooms "$count" "$seed"
            ;;
        events)
            check_container
            local count="${2:-10000}"
            local seed="${3:-42}"
            generate_events "$count" "$seed"
            ;;
        devices)
            check_container
            local count="${2:-1000}"
            local seed="${3:-42}"
            generate_devices "$count" "$seed"
            ;;
        preset)
            check_container
            local preset="${2:-small}"
            generate_preset "$preset"
            ;;
        cleanup)
            check_container
            cleanup_benchmark_data
            ;;
        stats)
            check_container
            show_stats
            ;;
        *)
            echo "Usage: $0 {users|rooms|events|devices|preset|cleanup|stats} [options]"
            echo ""
            echo "Commands:"
            echo "  users <count> [seed]     Generate benchmark users (default: 1000)"
            echo "  rooms <count> [seed]     Generate benchmark rooms (default: 100)"
            echo "  events <count> [seed]    Generate benchmark events (default: 10000)"
            echo "  devices <count> [seed]   Generate benchmark devices (default: 1000)"
            echo "  preset <size>            Generate preset dataset (small|medium|large)"
            echo "  cleanup                  Remove all benchmark data"
            echo "  stats                    Show benchmark data statistics"
            echo ""
            echo "Presets:"
            echo "  small   - 1K users, 100 rooms, 10K events, 2K devices"
            echo "  medium  - 10K users, 1K rooms, 100K events, 20K devices"
            echo "  large   - 100K users, 10K rooms, 1M events, 200K devices"
            echo ""
            echo "Examples:"
            echo "  $0 preset small"
            echo "  $0 users 5000 123"
            echo "  $0 cleanup"
            exit 1
            ;;
    esac
}

main "$@"
