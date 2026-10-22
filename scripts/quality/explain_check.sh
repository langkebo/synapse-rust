#!/usr/bin/env bash
# PERF-03/04/05 的 EXPLAIN 门禁：把「索引能不能被用上」钉成可失败断言。
#
# 存在理由：审计 W4 第一层的验收口径是**只能看 EXPLAIN**——测试通过不代表走了索引。
# `LOWER(name) LIKE '%x%'` 与建在原始 `name` 上的 trgm 索引是**两个不同的表达式**，
# 逻辑等价、计划天差地别；任何「功能测试」都察觉不到退化成顺序扫描。
#
# 用法：
#   TEST_DATABASE_URL=postgresql://synapse:synapse@localhost:55432/synapse_test \
#     bash scripts/quality/explain_check.sh
#   EXPLAIN_CHECK_EXISTING_DB=1 DATABASE_URL=<已灌好基线的库> \
#     bash scripts/quality/explain_check.sh     # CI 复用现成库，不 clone
# 退出码：0 = 三条关键查询都按预期用上索引；非 0 = 有退化（明细已在 stdout）。
#
# 实现要点：
#   * schema 来自 `scripts/test_db_template.sh`（与测试夹具同一套 base + 契约 SQL），
#     不手写 DDL，避免门禁验证的是"另一份 schema"。
#   * `SET enable_seqscan = off`：表是空的，规划器按代价本来就会选顺序扫描；
#     关掉它才能区分「索引存在但规划器嫌贵」与「索引根本用不上」——后者即使关掉
#     顺序扫描也只能 Seq Scan（实测确认，见 W4 回填）。
#   * 断言索引名/谓词形态，而不是断言"计划与某份快照逐字相同"：后者对 PG 版本敏感，
#     会把一次正常的小版本升级变成一次无意义的重录。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

DATABASE_URL="${DATABASE_URL:-${TEST_DATABASE_URL:-}}"
if [ -z "${DATABASE_URL}" ]; then
    echo "未提供连接串：设置 TEST_DATABASE_URL（本地测试库是 127.0.0.1:55432/synapse_test）。" >&2
    exit 2
fi
command -v psql >/dev/null 2>&1 || {
    echo "未找到 psql：把它放进 PATH。" >&2
    exit 2
}

SCRATCH_DB="synapse_explain_check_$$"
FAILED=0
MAINT_URL="$(printf '%s' "${DATABASE_URL}" | sed -E 's#/[^/?]+(\?|$)#/postgres\1#')"

# ── 源码形态断言 ─────────────────────────────────────────────────────────────
# EXPLAIN 只能证明"schema 支持这种写法"，证明不了"代码真的这么写"：
# 把 `name ILIKE $1` 改回 `LOWER(name) LIKE $1`，索引仍在、下面的计划断言仍绿。
# 所以先把三条查询的**写法**钉住（只查承载这三条查询的四个文件，不扫全仓）。
assert_source() {
    local title="$1" file="$2" pattern="$3"
    if grep -Eq "${pattern}" "${file}"; then
        echo "FAIL ${title}：${file} 仍出现 /${pattern}/" >&2
        FAILED=1
        return
    fi
    echo "ok   ${title}"
}

assert_source "PERF-03 房间目录谓词不再对列做 LOWER()" \
    synapse-storage/src/room/models.rs \
    'LOWER\(r\.(name|topic|canonical_alias)\) LIKE'
assert_source "PERF-03 用户房间搜索谓词不再对列做 LOWER()" \
    synapse-storage/src/room/mod.rs \
    'LOWER\((name|topic)\) LIKE'
assert_source "PERF-04 消息搜索不再对 content 做 LOWER()" \
    synapse-storage/src/event/search.rs \
    'LOWER\(content::text\) LIKE'
assert_source "PERF-05 成员服务器查询不再用前导通配 LIKE" \
    synapse-storage/src/membership/mod.rs \
    "user_id LIKE '%:%'"

cleanup() {
    psql "${MAINT_URL}" -q -v ON_ERROR_STOP=0 -c "DROP DATABASE IF EXISTS \"${SCRATCH_DB}\" WITH (FORCE)" >/dev/null 2>&1 || true
}

# 默认 clone 一份 scratch 库：本地测试库是共享的，门禁不该依赖它此刻的状态。
# CI 的 `schema-health-check` 车道已经把基线灌进一个专用库，设
# `EXPLAIN_CHECK_EXISTING_DB=1` 直接复用，省掉一次建库；此时绝不能挂 cleanup，
# 否则会把那个库一起删掉。
if [ "${EXPLAIN_CHECK_EXISTING_DB:-0}" = "1" ]; then
    CHECK_URL="${DATABASE_URL}"
else
    trap cleanup EXIT
    bash scripts/test_db_template.sh ensure >/dev/null
    bash scripts/test_db_template.sh clone "${SCRATCH_DB}" >/dev/null
    CHECK_URL="$(printf '%s' "${DATABASE_URL}" | sed -E "s#/[^/?]+(\\?|\$)#/${SCRATCH_DB}\\1#")"
fi

# 逐条断言可追加规划器开关（见 PERF-04：空表下规划器会优先选覆盖索引）。
PLAN_EXTRA_SETTINGS=()

plan_of() {
    local sql="$1"
    local args=(-tAX -v ON_ERROR_STOP=1 -c "SET enable_seqscan = off")
    local setting
    if [ "${#PLAN_EXTRA_SETTINGS[@]}" -gt 0 ]; then
        for setting in "${PLAN_EXTRA_SETTINGS[@]}"; do
            args+=(-c "SET ${setting}")
        done
    fi
    psql "${CHECK_URL}" "${args[@]}" -c "EXPLAIN (COSTS OFF) ${sql}"
}

# assert_plan <标题> <EXPLAIN 的 SQL> <必须出现的正则> [<不得出现的正则>]
assert_plan() {
    local title="$1" sql="$2" want="$3" unwanted="${4:-}"
    local plan
    plan="$(plan_of "${sql}")"
    PLAN_EXTRA_SETTINGS=()
    if ! grep -Eq "${want}" <<<"${plan}"; then
        echo "FAIL ${title}：计划里没有 /${want}/" >&2
        echo "${plan}" >&2
        FAILED=1
        return
    fi
    if [ -n "${unwanted}" ] && grep -Eq "${unwanted}" <<<"${plan}"; then
        echo "FAIL ${title}：计划里仍出现 /${unwanted}/" >&2
        echo "${plan}" >&2
        FAILED=1
        return
    fi
    echo "ok   ${title}"
    echo "${plan}" | sed 's/^/       /'
}

# PERF-03：房间目录搜索。`search_room_directory` / `count_public_rooms_matching` /
# `search_rooms_for_user` 都对原始列做 `ILIKE $1`，而 trgm 索引就建在这些列上。
assert_plan "PERF-03 rooms(name) trgm" \
    "SELECT room_id FROM rooms WHERE name ILIKE '%test%'" \
    "idx_rooms_name_trgm"
assert_plan "PERF-03 rooms(topic) trgm" \
    "SELECT room_id FROM rooms WHERE topic ILIKE '%test%'" \
    "idx_rooms_topic_trgm"
assert_plan "PERF-03 rooms(canonical_alias) trgm" \
    "SELECT room_id FROM rooms WHERE canonical_alias ILIKE '%test%'" \
    "idx_rooms_canonical_alias_trgm"

# PERF-04：消息搜索。`jsonb_path_ops` 只服务 `@>`，子串匹配必须走 trgm 表达式索引。
# 空表下 `idx_events_sync_covering`（Index Only Scan，不回表）比 trgm 的 Bitmap Heap Scan
# 便宜，规划器会先选它——那是代价差异、不是索引不可用。故这里额外关掉普通/仅索引扫描，
# 把候选收窄到「顺序扫描 vs 位图索引扫描」，才能证明 `idx_events_content_trgm` 真的可用。
PLAN_EXTRA_SETTINGS=("enable_indexscan = off" "enable_indexonlyscan = off")
assert_plan "PERF-04 events(content::text) trgm" \
    "SELECT event_id FROM events WHERE content::text ILIKE '%test%'" \
    "idx_events_content_trgm"

# PERF-05：成员所在服务器。`user_id LIKE '%:%'` 是前导通配，行集本来就被
# (room_id, membership) 收窄 ⇒ 断言的是「不再有 LIKE 谓词」而不是「走索引」。
assert_plan "PERF-05 room_memberships 无前导通配 LIKE" \
    "SELECT DISTINCT SUBSTRING(user_id FROM POSITION(':' IN user_id) + 1) FROM room_memberships WHERE room_id = 'x' AND membership = 'join' AND POSITION(':' IN user_id) > 0" \
    "idx_room_memberships" \
    "%:%'::text\\)"

if [ "${FAILED}" -ne 0 ]; then
    echo "❌ EXPLAIN 门禁未通过（见上）。" >&2
    exit 1
fi
echo "✅ 三条关键查询均按预期命中索引。"
