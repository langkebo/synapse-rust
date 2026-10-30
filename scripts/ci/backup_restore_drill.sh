#!/usr/bin/env bash
# =============================================================================
# 备份/恢复演练（P1-4）
# =============================================================================
# 目的：证明"备份文件真的能恢复"，而不只是"pg_dump 退出码为 0"。
#
# 审计报告里对备份链路的结论是：`scripts/backup_database.sh` 只校验到
# `pg_restore --list`（能读出目录），**从未真正恢复到另一个库并比对内容**。
# 也就是说"备份可用"一直是推定，不是实测：空库 dump、缺索引、丢序列、
# 版本不兼容等失效形态全都能通过 `--list`。
#
# 本脚本做的是端到端演练：
#   ① 往源库灌入可校验的种子数据（restore_drill_probe）
#   ② pg_dump -Fc 出备份文件，校验体积下限
#   ③ 用 pg_restore 恢复到**另一个库**
#   ④ 逐项比对：表行数 / 索引定义 / 约束 / 触发器 / 序列值 / 扩展
#   ⑤ 任一项不一致即非零退出（fail-closed）
#
# 设计约束：
#   * 只依赖 psql/pg_dump/pg_restore（客户端工具），**不依赖 docker exec**，
#     因此 CI 的 service 容器与真实实例都能用；
#   * 比对逻辑**不写死具体表名**，全部从 catalog 现算 —— schema 增删表不需要改本脚本；
#   * 对象数有下限（MIN_TABLES/MIN_INDEXES），防止"dump 到空库也算通过"。
#
# 用法：
#   PGHOST=127.0.0.1 PGUSER=synapse PGPASSWORD=synapse \
#     SOURCE_DB=synapse_drill TARGET_DB=synapse_restore_drill \
#     ./scripts/ci/backup_restore_drill.sh
#
# 环境变量：
#   PGHOST/PGPORT/PGUSER/PGPASSWORD - libpq 标准变量（PGDATABASE 不使用）
#   SOURCE_DB     - 源库（演练前应已灌好 schema）
#   TARGET_DB     - 恢复目标库（脚本会先 drop 再 create）
#   WORK_DIR      - 产物目录（dump 文件与指纹），默认 ./artifacts/restore-drill
#   SEED_ROWS     - 种子行数，默认 5000
#   MIN_DUMP_BYTES- dump 体积下限，默认 65536（自定义格式的真实 schema 远大于此）
#   MIN_TABLES    - public 表数下限，默认 200
#   MIN_INDEXES   - public 索引数下限，默认 300
# =============================================================================
set -euo pipefail

PGHOST="${PGHOST:-127.0.0.1}"
PGPORT="${PGPORT:-5432}"
PGUSER="${PGUSER:-synapse}"
SOURCE_DB="${SOURCE_DB:-synapse_drill}"
TARGET_DB="${TARGET_DB:-synapse_restore_drill}"
WORK_DIR="${WORK_DIR:-./artifacts/restore-drill}"
SEED_ROWS="${SEED_ROWS:-5000}"
MIN_DUMP_BYTES="${MIN_DUMP_BYTES:-65536}"
MIN_TABLES="${MIN_TABLES:-200}"
MIN_INDEXES="${MIN_INDEXES:-300}"

DUMP_FILE="$WORK_DIR/synapse_drill.dump"
SRC_FINGERPRINT="$WORK_DIR/source.fingerprint"
DST_FINGERPRINT="$WORK_DIR/restored.fingerprint"

log() { echo "[drill] $*"; }
fail() {
    echo "[drill][FAIL] $*" >&2
    exit 1
}

# 统一的 psql 调用：-X 忽略 psqlrc，-q 静音，-A -t 输出纯值，-v ON_ERROR_STOP=1 失败即退
q() {
    local db="$1"
    shift
    psql -X -q -A -t -v ON_ERROR_STOP=1 \
        -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$db" "$@"
}

# -----------------------------------------------------------------------------
# 指纹：把"结构 + 数据"压成可 diff 的文本
# -----------------------------------------------------------------------------
# 表行数：从 catalog 现算每张表的 count(*)，不写死表名。
table_counts() {
    local db="$1" gen
    gen="$(q "$db" -c "
        SELECT string_agg(format('SELECT %L AS obj, count(*) AS rows FROM public.%I', relname, relname),
                          ' UNION ALL ' ORDER BY relname)
        FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind IN ('r', 'p') AND n.nspname = 'public'")"
    [ -n "$gen" ] || fail "$db 的 public schema 里没有任何表（schema 未灌入？）"
    q "$db" -c "SELECT * FROM ($gen) AS t ORDER BY 1"
}

index_defs() {
    q "$1" -c "SELECT indexname || ' :: ' || indexdef
               FROM pg_indexes WHERE schemaname = 'public' ORDER BY indexname"
}

constraint_defs() {
    q "$1" -c "SELECT c.relname || ' :: ' || con.conname || ' :: ' || pg_get_constraintdef(con.oid)
               FROM pg_constraint con
               JOIN pg_class c ON c.oid = con.conrelid
               JOIN pg_namespace n ON n.oid = c.relnamespace
               WHERE n.nspname = 'public' ORDER BY 1"
}

trigger_defs() {
    q "$1" -c "SELECT c.relname || ' :: ' || tg.tgname || ' :: ' || pg_get_triggerdef(tg.oid)
               FROM pg_trigger tg
               JOIN pg_class c ON c.oid = tg.tgrelid
               JOIN pg_namespace n ON n.oid = c.relnamespace
               WHERE NOT tg.tgisinternal AND n.nspname = 'public' ORDER BY 1"
}

sequence_values() {
    q "$1" -c "SELECT sequencename || ' :: ' || COALESCE(last_value::text, 'null')
               FROM pg_sequences WHERE schemaname = 'public' ORDER BY sequencename"
}

extension_list() {
    q "$1" -c "SELECT extname FROM pg_extension ORDER BY extname"
}

# 生成完整指纹（节标题必须固定，便于 diff 定位）
fingerprint() {
    local db="$1" out="$2"
    {
        echo "## extensions"
        extension_list "$db"
        echo "## tables"
        table_counts "$db"
        echo "## indexes"
        index_defs "$db"
        echo "## constraints"
        constraint_defs "$db"
        echo "## triggers"
        trigger_defs "$db"
        echo "## sequences"
        sequence_values "$db"
    } >"$out"
}

# -----------------------------------------------------------------------------
# 主流程
# -----------------------------------------------------------------------------
main() {
    mkdir -p "$WORK_DIR"
    log "source=$SOURCE_DB target=$TARGET_DB work_dir=$WORK_DIR"

    # 0) 前置：客户端主版本必须**等于**服务端主版本。
    #
    # 原来写的是 `>=`，理由只覆盖了 dump 方向（"pg_dump 不会 dump 比自己新的服务端"）。
    # 但本演练是在**同一台服务端**上 dump 再 restore，反方向不成立：
    # pg_dump 18 对 PG16 写出的 dump 里带 `SET transaction_timeout = 0;`
    # （PG17 才引入的 GUC），pg_restore 回到这台 PG16 时直接
    #   ERROR:  unrecognized configuration parameter "transaction_timeout"
    # 实测 2026-10-30 CI run 40996685357：runner 自带 pg_dump 18.6 + service postgres:16，
    # 正是这个组合红的 —— 而 `>=` 守卫放行了 18。
    # 生产侧没踩是因为 `scripts/backup_database.sh` 用 `docker exec <pg> pg_dump`，
    # 客户端与服务端天然同版本；CI 只能用 runner 的客户端，所以必须两边都钉住。
    local client_major server_major
    client_major="$(pg_dump --version | grep -oE '[0-9]+' | head -1)"
    server_major="$(q "$SOURCE_DB" -c "SHOW server_version_num")"
    server_major=$((server_major / 10000))
    log "client major=$client_major server major=$server_major"
    [ "$client_major" -eq "$server_major" ] ||
        fail "pg_dump 主版本 $client_major ≠ 服务端 $server_major：dump 与 restore 必须在同一主版本上做（更新的客户端会写出服务端不认识的 GUC）"

    # 1) 种子数据：一张独立的探针表 + 索引 + 序列，用于验证"数据真的被恢复"
    log "seeding $SEED_ROWS rows into restore_drill_probe ..."
    q "$SOURCE_DB" -v seed="$SEED_ROWS" -v ON_ERROR_STOP=1 <<'SQL'
DROP TABLE IF EXISTS public.restore_drill_probe;
CREATE TABLE public.restore_drill_probe (
    id BIGSERIAL PRIMARY KEY,
    payload TEXT NOT NULL,
    created_ts BIGINT NOT NULL
);
INSERT INTO public.restore_drill_probe (payload, created_ts)
SELECT md5(g::text), 1700000000000 + g FROM generate_series(1, :seed) AS g;
CREATE INDEX idx_restore_drill_probe_payload ON public.restore_drill_probe(payload);
SQL

    # 2) 源库指纹
    fingerprint "$SOURCE_DB" "$SRC_FINGERPRINT"

    local src_tables src_indexes
    src_tables="$(q "$SOURCE_DB" -c "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relkind IN ('r','p') AND n.nspname = 'public'")"
    src_indexes="$(q "$SOURCE_DB" -c "SELECT count(*) FROM pg_indexes WHERE schemaname = 'public'")"
    log "source: $src_tables tables, $src_indexes indexes"
    [ "$src_tables" -ge "$MIN_TABLES" ] || fail "源库表数 $src_tables < 下限 $MIN_TABLES（schema 未灌入？）"
    [ "$src_indexes" -ge "$MIN_INDEXES" ] || fail "源库索引数 $src_indexes < 下限 $MIN_INDEXES（schema 未灌入？）"

    # 3) 备份：自定义格式（与生产 scripts/backup_database.sh 的 `pg_dump -Fc` 同形态）
    log "pg_dump -Fc -> $DUMP_FILE"
    pg_dump -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$SOURCE_DB" -Fc -b \
        --file="$DUMP_FILE"

    local dump_bytes
    dump_bytes="$(wc -c <"$DUMP_FILE" | tr -d '[:space:]')"
    [ "$dump_bytes" -ge "$MIN_DUMP_BYTES" ] || fail "dump 体积 ${dump_bytes}B < ${MIN_DUMP_BYTES}B（疑似空备份）"
    log "dump size: ${dump_bytes}B"

    # `--list` 只是最低门槛（生产备份脚本已有的检查），这里保留并与后续真恢复形成对照
    local list_entries
    list_entries="$(pg_restore --list "$DUMP_FILE" | grep -vc '^;' || true)"
    [ "$list_entries" -gt 0 ] || fail "pg_restore --list 没有列出任何对象"
    log "pg_restore --list entries: $list_entries"

    # 4) 恢复到另一个库（先清空，保证可重复执行）
    log "recreating $TARGET_DB ..."
    psql -X -q -v ON_ERROR_STOP=1 -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d postgres \
        -c "DROP DATABASE IF EXISTS \"$TARGET_DB\"" \
        -c "CREATE DATABASE \"$TARGET_DB\""
    log "pg_restore -> $TARGET_DB"
    pg_restore -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$TARGET_DB" \
        --no-owner --no-privileges --exit-on-error "$DUMP_FILE"

    # 5) 恢复后指纹 + 逐项比对
    fingerprint "$TARGET_DB" "$DST_FINGERPRINT"
    if ! diff -u "$SRC_FINGERPRINT" "$DST_FINGERPRINT" >"$WORK_DIR/fingerprint.diff"; then
        echo "-------- 指纹差异（源库 vs 恢复库） --------" >&2
        head -200 "$WORK_DIR/fingerprint.diff" >&2
        fail "恢复结果与源库不一致（见 $WORK_DIR/fingerprint.diff）"
    fi

    # 6) 种子数据专项断言：行数 + 内容校验和（防止"表在但数据没回来"）
    local probe_src probe_dst
    probe_src="$(q "$SOURCE_DB" -c "SELECT count(*) || ':' || COALESCE(md5(string_agg(payload, ',' ORDER BY id)), '-') FROM public.restore_drill_probe")"
    probe_dst="$(q "$TARGET_DB" -c "SELECT count(*) || ':' || COALESCE(md5(string_agg(payload, ',' ORDER BY id)), '-') FROM public.restore_drill_probe")"
    [ "$probe_src" = "$probe_dst" ] || fail "探针表内容不一致: source=$probe_src restored=$probe_dst"
    log "probe table verified: $probe_dst"

    local dst_tables dst_indexes
    dst_tables="$(q "$TARGET_DB" -c "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relkind IN ('r','p') AND n.nspname = 'public'")"
    dst_indexes="$(q "$TARGET_DB" -c "SELECT count(*) FROM pg_indexes WHERE schemaname = 'public'")"

    psql -X -q -v ON_ERROR_STOP=1 -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d postgres \
        -c "DROP DATABASE IF EXISTS \"$TARGET_DB\"" >/dev/null

    {
        echo "### Backup restore drill"
        echo ""
        echo "| 项 | 值 |"
        echo "|----|----|"
        echo "| dump 体积 | ${dump_bytes} B |"
        echo "| pg_restore --list 条目 | ${list_entries} |"
        echo "| 表数（源 / 恢复） | ${src_tables} / ${dst_tables} |"
        echo "| 索引数（源 / 恢复） | ${src_indexes} / ${dst_indexes} |"
        echo "| 探针表行数:校验和 | ${probe_dst} |"
        echo "| 结论 | 备份可恢复，结构与数据逐项一致 |"
    } >>"${GITHUB_STEP_SUMMARY:-/dev/null}"

    log "OK: 备份 → 恢复 → 逐项比对全部通过"
}

main "$@"
