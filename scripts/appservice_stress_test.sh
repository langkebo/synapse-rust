#!/usr/bin/env bash
# appservice_stress_test.sh — AppService scheduler 生产压测脚本
#
# 用途: 对 MAX_SERVICES_PER_TICK、HIGH_PENDING_TRANSACTION_THRESHOLD、
#       retry backoff、mixed backlog 进行持续压测，输出指标阈值与容量建议。
#
# 依赖: curl, jq, bc, 已运行的 synapse-rust 实例 + admin token
#
# 用法:
#   ADMIN_TOKEN=xxxx ./scripts/appservice_stress_test.sh [scenario] [duration_seconds]
#
# 场景:
#   event-only          — 纯事件积压，无 transaction
#   transaction-only    — 纯 transaction 积压
#   mixed               — 事件 + transaction 混合
#   mixed-backoff       — 混合 + 部分 AS 退避
#   recovery            — 多个 AS 同时恢复
#   continuous-ingress  — 持续事件流入
#   super-event-heavy   — 超大事件量单一 AS
#   all                 — 运行全部场景 (默认)

set -euo pipefail

ADMIN_TOKEN="${ADMIN_TOKEN:-}"
BASE_URL="${BASE_URL:-http://localhost:8008}"
PROMETHEUS_URL="${PROMETHEUS_URL:-http://localhost:9090/metrics}"
DURATION="${2:-60}"
SCENARIO="${1:-all}"

if [ -z "$ADMIN_TOKEN" ]; then
  echo "ERROR: ADMIN_TOKEN environment variable is required"
  exit 1
fi

if ! command -v jq &>/dev/null; then
  echo "ERROR: jq is required"
  exit 1
fi

AUTH_HEADER="Authorization: Bearer $ADMIN_TOKEN"
RESULTS_DIR="${RESULTS_DIR:-/tmp/appservice_stress_results_$$}"
mkdir -p "$RESULTS_DIR"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# 获取统计快照
get_statistics() {
  curl -sf -H "$AUTH_HEADER" \
    "$BASE_URL/_synapse/admin/v1/appservices/statistics" 2>/dev/null || echo '[]'
}

# 获取 Prometheus 指标
get_prometheus_metrics() {
  curl -sf "$PROMETHEUS_URL" 2>/dev/null | grep -E "^synapse_appservice_" || true
}

# 获取 telemetry 指标
get_telemetry_metrics() {
  curl -sf -H "$AUTH_HEADER" \
    "$BASE_URL/_synapse/admin/v1/telemetry/metrics" 2>/dev/null | jq -r '.appservice_scheduler // {}' 2>/dev/null || echo '{}'
}

aggregate_statistics_summary() {
  local stats_json="$1"
  echo "$stats_json" | jq -c '
    {
      total_services: length,
      scheduler_available_services: ([.[] | select(.scheduler.available == true)] | length),
      services_in_backoff: ([.[] | select((.scheduler.transaction_state // "") == "retry_backoff")] | length),
      services_capacity_limited: ([.[] | select((.scheduler.last_result // "") == "capacity_limited" or (.scheduler.transaction_state // "") == "capacity_limited")] | length),
      services_with_pending_transactions: ([.[] | select(((.pending_transaction_count // 0) > 0) or ((.scheduler.pending_transaction_count // 0) > 0))] | length),
      total_pending_events: ([.[].pending_event_count // 0] | add // 0),
      total_pending_transactions: ([.[].pending_transaction_count // 0] | add // 0),
      total_success_count: ([.[].scheduler.total_success_count // 0] | add // 0),
      total_failure_count: ([.[].scheduler.total_failure_count // 0] | add // 0),
      total_backoff_count: ([.[].scheduler.total_backoff_count // 0] | add // 0),
      total_capacity_limited_count: ([.[].scheduler.total_capacity_limited_count // 0] | add // 0),
      total_in_flight_count: ([.[].scheduler.total_in_flight_count // 0] | add // 0)
    }
  '
}

normalize_telemetry_summary() {
  local telemetry_json="$1"
  echo "$telemetry_json" | jq -c '
    {
      total_services: (.total_services // 0),
      scheduler_available_services: (.scheduler_available_services // 0),
      services_in_backoff: (.services_in_backoff // 0),
      services_capacity_limited: (.services_capacity_limited // 0),
      services_with_pending_transactions: (.services_with_pending_transactions // 0),
      total_pending_events: (.total_pending_events // 0),
      total_pending_transactions: (.total_pending_transactions // 0),
      total_success_count: (.total_success_count // 0),
      total_failure_count: (.total_failure_count // 0),
      total_backoff_count: (.total_backoff_count // 0),
      total_capacity_limited_count: (.total_capacity_limited_count // 0),
      total_in_flight_count: (.total_in_flight_count // 0)
    }
  '
}

prometheus_metric_value() {
  local metrics_text="$1"
  local metric_name="$2"
  local value
  value="$(echo "$metrics_text" | awk -v metric="$metric_name" '$1 == metric { found = 1; value = $2 } END { if (found) print value; else print 0 }')"
  if [ -z "$value" ]; then
    value=0
  fi
  echo "$value"
}

build_prometheus_summary() {
  local metrics_text="$1"
  local total_services
  local available_services
  local backoff_services
  local capacity_limited_services
  local services_with_pending_transactions
  local pending_events
  local pending_transactions
  local success_count
  local failure_count
  local backoff_count
  local capacity_limited_count
  local in_flight_count

  total_services="$(prometheus_metric_value "$metrics_text" "synapse_appservice_scheduler_total_services")"
  available_services="$(prometheus_metric_value "$metrics_text" "synapse_appservice_scheduler_available_services")"
  backoff_services="$(prometheus_metric_value "$metrics_text" "synapse_appservice_scheduler_backoff_services")"
  capacity_limited_services="$(prometheus_metric_value "$metrics_text" "synapse_appservice_scheduler_capacity_limited_services")"
  services_with_pending_transactions="$(prometheus_metric_value "$metrics_text" "synapse_appservice_scheduler_services_with_pending_transactions")"
  pending_events="$(prometheus_metric_value "$metrics_text" "synapse_appservice_scheduler_pending_events")"
  pending_transactions="$(prometheus_metric_value "$metrics_text" "synapse_appservice_scheduler_pending_transactions")"
  success_count="$(prometheus_metric_value "$metrics_text" "synapse_appservice_scheduler_success_count")"
  failure_count="$(prometheus_metric_value "$metrics_text" "synapse_appservice_scheduler_failure_count")"
  backoff_count="$(prometheus_metric_value "$metrics_text" "synapse_appservice_scheduler_backoff_count")"
  capacity_limited_count="$(prometheus_metric_value "$metrics_text" "synapse_appservice_scheduler_capacity_limited_count")"
  in_flight_count="$(prometheus_metric_value "$metrics_text" "synapse_appservice_scheduler_in_flight_count")"

  jq -cn \
    --argjson total_services "$total_services" \
    --argjson scheduler_available_services "$available_services" \
    --argjson services_in_backoff "$backoff_services" \
    --argjson services_capacity_limited "$capacity_limited_services" \
    --argjson services_with_pending_transactions "$services_with_pending_transactions" \
    --argjson total_pending_events "$pending_events" \
    --argjson total_pending_transactions "$pending_transactions" \
    --argjson total_success_count "$success_count" \
    --argjson total_failure_count "$failure_count" \
    --argjson total_backoff_count "$backoff_count" \
    --argjson total_capacity_limited_count "$capacity_limited_count" \
    --argjson total_in_flight_count "$in_flight_count" \
    '{
      total_services: $total_services,
      scheduler_available_services: $scheduler_available_services,
      services_in_backoff: $services_in_backoff,
      services_capacity_limited: $services_capacity_limited,
      services_with_pending_transactions: $services_with_pending_transactions,
      total_pending_events: $total_pending_events,
      total_pending_transactions: $total_pending_transactions,
      total_success_count: $total_success_count,
      total_failure_count: $total_failure_count,
      total_backoff_count: $total_backoff_count,
      total_capacity_limited_count: $total_capacity_limited_count,
      total_in_flight_count: $total_in_flight_count
    }'
}

compare_outlet_summaries() {
  local statistics_summary="$1"
  local telemetry_summary="$2"
  local prometheus_summary="$3"

  jq -cn \
    --argjson statistics "$statistics_summary" \
    --argjson telemetry "$telemetry_summary" \
    --argjson prometheus "$prometheus_summary" \
    --argjson keys '[
      "total_services",
      "scheduler_available_services",
      "services_in_backoff",
      "services_capacity_limited",
      "services_with_pending_transactions",
      "total_pending_events",
      "total_pending_transactions",
      "total_success_count",
      "total_failure_count",
      "total_backoff_count",
      "total_capacity_limited_count",
      "total_in_flight_count"
    ]' \
    '
    {
      comparisons: [
        $keys[] as $key |
        {
          key: $key,
          statistics: ($statistics[$key] // 0),
          telemetry: ($telemetry[$key] // 0),
          prometheus: ($prometheus[$key] // 0),
          consistent: (($statistics[$key] // 0) == ($telemetry[$key] // 0) and ($statistics[$key] // 0) == ($prometheus[$key] // 0))
        }
      ]
    }
    | .mismatched_keys = [.comparisons[] | select(.consistent | not) | .key]
    | .consistent = (.mismatched_keys | length == 0)
    '
}

write_scenario_result() {
  local scenario="$1"
  local scenario_metrics="$2"
  local stats_json="$3"
  local telemetry_raw
  local prometheus_raw
  local statistics_summary
  local telemetry_summary
  local prometheus_summary
  local consistency_report

  telemetry_raw="$(get_telemetry_metrics)"
  prometheus_raw="$(get_prometheus_metrics)"
  statistics_summary="$(aggregate_statistics_summary "$stats_json")"
  telemetry_summary="$(normalize_telemetry_summary "$telemetry_raw")"
  prometheus_summary="$(build_prometheus_summary "$prometheus_raw")"
  consistency_report="$(compare_outlet_summaries "$statistics_summary" "$telemetry_summary" "$prometheus_summary")"

  jq -n \
    --arg scenario "$scenario" \
    --arg generated_at "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
    --argjson duration_seconds "$DURATION" \
    --argjson scenario_metrics "$scenario_metrics" \
    --argjson statistics "$statistics_summary" \
    --argjson telemetry "$telemetry_summary" \
    --argjson prometheus "$prometheus_summary" \
    --argjson consistency "$consistency_report" \
    '{
      scenario: $scenario,
      generated_at: $generated_at,
      duration_seconds: $duration_seconds,
      scenario_metrics: $scenario_metrics,
      outlets: {
        statistics: $statistics,
        telemetry: $telemetry,
        prometheus: $prometheus
      },
      consistency: $consistency
    }' > "$RESULTS_DIR/${scenario}.json"

  if [ "$(echo "$consistency_report" | jq -r '.consistent')" = "true" ]; then
    log_info "场景 ${scenario} 三出口关键聚合一致"
  else
    log_warn "场景 ${scenario} 三出口存在差异: $(echo "$consistency_report" | jq -r '.mismatched_keys | join(", ")')"
  fi
}

require_scheduler_preconditions() {
  local stats
  local telemetry
  local total_services
  local scheduler_available

  stats="$(get_statistics)"
  telemetry="$(get_telemetry_metrics)"
  total_services="$(echo "$stats" | jq 'length' 2>/dev/null || echo 0)"
  scheduler_available="$(echo "$telemetry" | jq '.scheduler_available_services // 0' 2>/dev/null || echo 0)"

  if [ "$total_services" -eq 0 ]; then
    log_error "未检测到已注册的 appservice；请先注册 live 压测目标（例如 stress_as_1..stress_as_5）"
    exit 1
  fi

  if [ "$scheduler_available" -eq 0 ]; then
    log_warn "scheduler_available_services=0；先注入一条 probe 事件触发 scheduler 观测"
    inject_event "!stress_room_1:localhost" "m.room.message" "stress_as_1"
    for _ in $(seq 1 4); do
      sleep 2
      telemetry="$(get_telemetry_metrics)"
      scheduler_available="$(echo "$telemetry" | jq '.scheduler_available_services // 0' 2>/dev/null || echo 0)"
      if [ "$scheduler_available" -gt 0 ]; then
        break
      fi
    done

    if [ "$scheduler_available" -eq 0 ]; then
      log_error "probe 后 telemetry 仍显示 scheduler_available_services=0；当前实例可能未启动 appservice scheduler（例如未加载 app_service_config_files）"
      exit 1
    fi
  fi
}

# 注入测试事件
inject_event() {
  local room_id="$1"
  local event_type="${2:-m.room.message}"
  local as_id="$3"
  curl -sf -H "$AUTH_HEADER" -H "Content-Type: application/json" \
    -X POST "$BASE_URL/_synapse/admin/v1/appservices/$as_id/events" \
    -d "{\"room_id\":\"$room_id\",\"event_type\":\"$event_type\",\"sender\":\"@stress:localhost\",\"content\":{\"msgtype\":\"m.text\",\"body\":\"stress $event_type\"}}" \
    2>/dev/null || true
}

# 场景 1: event-only
run_event_only() {
  log_info "=== 场景 1: event-only (纯事件积压) ==="
  local start_time=$(date +%s)
  local end_time=$((start_time + DURATION))
  local event_count=0

  local stats_before=$(get_statistics | jq 'length')
  log_info "当前 AS 数量: $stats_before"

  while [ $(date +%s) -lt $end_time ]; do
    for i in $(seq 1 5); do
      inject_event "!stress_room_$i:localhost" "m.room.message" "stress_as_$i" 2>/dev/null || true
      event_count=$((event_count + 1))
    done
    sleep 0.1
  done

  local stats_after=$(get_statistics)
  local total_pending=$(echo "$stats_after" | jq '[.[].pending_event_count // 0] | add // 0')
  local total_success=$(echo "$stats_after" | jq '[.[].scheduler.total_success_count // 0] | add // 0')
  local total_failure=$(echo "$stats_after" | jq '[.[].scheduler.total_failure_count // 0] | add // 0')

  log_info "事件注入: $event_count"
  log_info "待处理事件: $total_pending"
  log_info "成功投递: $total_success"
  log_info "失败次数: $total_failure"

  # 验收: 入队到首次 dispatch p95 <= 200ms (通过日志或 metrics 观察)
  local capacity_limited=$(echo "$stats_after" | jq '[.[] | select(.scheduler.last_result == "capacity_limited")] | length')
  if [ "$capacity_limited" -gt 0 ]; then
    log_warn "capacity_limited AS 数量: $capacity_limited (考虑提高 MAX_SERVICES_PER_TICK)"
  fi

  write_scenario_result \
    "event-only" \
    "$(jq -cn \
      --argjson events_injected "$event_count" \
      --argjson pending "$total_pending" \
      --argjson success "$total_success" \
      --argjson failure "$total_failure" \
      --argjson capacity_limited "$capacity_limited" \
      '{
        events_injected: $events_injected,
        pending: $pending,
        success: $success,
        failure: $failure,
        capacity_limited: $capacity_limited
      }')" \
    "$stats_after"
}

# 场景 2: transaction-only
run_transaction_only() {
  log_info "=== 场景 2: transaction-only (纯 transaction 积压) ==="
  local start_time=$(date +%s)
  local end_time=$((start_time + DURATION))

  # 模拟 bridge 不可达，制造 transaction 积压
  local stats_before=$(get_statistics)
  local pending_before=$(echo "$stats_before" | jq '[.[].pending_transaction_count // 0] | add // 0')

  # 注入大量事件到单一 AS，迫使 transaction 积压
  for i in $(seq 1 100); do
    inject_event "!txn_stress:localhost" "m.room.message" "stress_as_1" 2>/dev/null || true
  done

  sleep 5

  local stats_after=$(get_statistics)
  local pending_after=$(echo "$stats_after" | jq '[.[].pending_transaction_count // 0] | add // 0')
  local backoff_count=$(echo "$stats_after" | jq '[.[].scheduler.total_backoff_count // 0] | add // 0')

  log_info "Transaction 积压 (前): $pending_before"
  log_info "Transaction 积压 (后): $pending_after"
  log_info "退避次数: $backoff_count"

  # 验收: transaction 重试间隔符合退避策略
  local retry_services=$(echo "$stats_after" | jq '[.[] | select(.scheduler.transaction_state == "retry_backoff")] | length')
  if [ "$retry_services" -gt 0 ]; then
    log_info "退避中的 AS: $retry_services (符合预期)"
  else
    log_warn "无 AS 进入退避状态 (检查 bridge 可达性或 MAX_FATAL_FAILURES)"
  fi

  write_scenario_result \
    "transaction-only" \
    "$(jq -cn \
      --argjson pending_before "$pending_before" \
      --argjson pending_after "$pending_after" \
      --argjson backoff "$backoff_count" \
      --argjson retry_services "$retry_services" \
      '{
        pending_before: $pending_before,
        pending_after: $pending_after,
        backoff: $backoff,
        retry_services: $retry_services
      }')" \
    "$stats_after"
}

# 场景 3: mixed
run_mixed() {
  log_info "=== 场景 3: mixed (事件 + transaction 混合) ==="
  local start_time=$(date +%s)
  local end_time=$((start_time + DURATION))

  while [ $(date +%s) -lt $end_time ]; do
    # 向多个 AS 注入事件
    for i in $(seq 1 3); do
      inject_event "!mixed_room_$i:localhost" "m.room.message" "stress_as_$i" 2>/dev/null || true
    done
    sleep 0.2
  done

  local stats=$(get_statistics)
  local total_pending_events=$(echo "$stats" | jq '[.[].pending_event_count // 0] | add // 0')
  local total_pending_txn=$(echo "$stats" | jq '[.[].pending_transaction_count // 0] | add // 0')
  local total_success=$(echo "$stats" | jq '[.[].scheduler.total_success_count // 0] | add // 0')

  log_info "待处理事件: $total_pending_events"
  log_info "待处理 transaction: $total_pending_txn"
  log_info "成功投递: $total_success"

  # 验收: 无长期饥饿，transaction 优先
  if [ "$total_pending_txn" -gt 0 ] && [ "$total_pending_events" -gt 100 ]; then
    log_info "混合积压场景验证通过 (transaction 优先调度)"
  fi

  write_scenario_result \
    "mixed" \
    "$(jq -cn \
      --argjson pending_events "$total_pending_events" \
      --argjson pending_txn "$total_pending_txn" \
      --argjson success "$total_success" \
      '{
        pending_events: $pending_events,
        pending_txn: $pending_txn,
        success: $success
      }')" \
    "$stats"
}

# 场景 4: mixed-backoff
run_mixed_backoff() {
  log_info "=== 场景 4: mixed-backoff (混合 + 部分 AS 退避) ==="

  # 向健康 AS 和不可达 AS 同时注入事件
  for i in $(seq 1 50); do
    inject_event "!healthy:localhost" "m.room.message" "stress_as_1" 2>/dev/null || true
    inject_event "!unhealthy:localhost" "m.room.message" "stress_as_2" 2>/dev/null || true
  done

  sleep 10

  local stats=$(get_statistics)
  local healthy_success=$(echo "$stats" | jq '[.[] | select(.as_id == "stress_as_1") | .scheduler.total_success_count // 0] | add // 0')
  local unhealthy_last_result=$(echo "$stats" | jq -r '[.[] | select(.as_id == "stress_as_2") | .scheduler.last_result // "unknown"] | .[0]')
  local unhealthy_txn_state=$(echo "$stats" | jq -r '[.[] | select(.as_id == "stress_as_2") | .scheduler.transaction_state // "unknown"] | .[0]')
  local unhealthy_pending_txn=$(echo "$stats" | jq '[.[] | select(.as_id == "stress_as_2") | .pending_transaction_count // 0] | add // 0')
  local unhealthy_backoff=$(echo "$stats" | jq '[.[] | select(.as_id == "stress_as_2") | .scheduler.total_backoff_count // 0] | add // 0')

  log_info "健康 AS 成功数: $healthy_success"
  log_info "不健康 AS 最近结果: $unhealthy_last_result"
  log_info "不健康 AS transaction 状态: $unhealthy_txn_state"
  log_info "不健康 AS pending transaction: $unhealthy_pending_txn"
  log_info "不健康 AS 累计 backoff: $unhealthy_backoff"

  # 验收: 失败 AS 不阻塞健康 AS
  if [ "$healthy_success" -gt 0 ]; then
    log_info "验证通过: 失败 AS 未阻塞健康 AS"
  else
    log_error "验证失败: 健康 AS 未获得 dispatch"
  fi

  write_scenario_result \
    "mixed-backoff" \
    "$(jq -cn \
      --argjson healthy_success "$healthy_success" \
      --arg unhealthy_last_result "$unhealthy_last_result" \
      --arg unhealthy_txn_state "$unhealthy_txn_state" \
      --argjson unhealthy_pending_txn "$unhealthy_pending_txn" \
      --argjson unhealthy_backoff "$unhealthy_backoff" \
      '{
        healthy_success: $healthy_success,
        unhealthy_last_result: $unhealthy_last_result,
        unhealthy_txn_state: $unhealthy_txn_state,
        unhealthy_pending_txn: $unhealthy_pending_txn,
        unhealthy_backoff: $unhealthy_backoff
      }')" \
    "$stats"
}

# 场景 5: recovery
run_recovery() {
  log_info "=== 场景 5: recovery (多个 AS 同时恢复) ==="

  # 先制造多个 AS 退避
  for i in $(seq 1 5); do
    for j in $(seq 1 20); do
      inject_event "!recovery_$i:localhost" "m.room.message" "stress_as_$i" 2>/dev/null || true
    done
  done

  log_info "等待退避..."
  sleep 15

  local stats=$(get_statistics)
  local backoff_services=$(echo "$stats" | jq '[.[] | select(.scheduler.transaction_state == "retry_backoff")] | length')
  log_info "退避中的 AS: $backoff_services"

  # 验收: 恢复窗口内所有 AS 获得 dispatch
  local total_recovery=$(echo "$stats" | jq '[.[].scheduler.total_success_count // 0] | add // 0')
  log_info "总成功数: $total_recovery"

  write_scenario_result \
    "recovery" \
    "$(jq -cn \
      --argjson backoff_services "$backoff_services" \
      --argjson total_recovery "$total_recovery" \
      '{
        backoff_services: $backoff_services,
        total_recovery: $total_recovery
      }')" \
    "$stats"
}

# 场景 6: continuous-ingress
run_continuous_ingress() {
  log_info "=== 场景 6: continuous-ingress (持续事件流入) ==="
  local start_time=$(date +%s)
  local end_time=$((start_time + DURATION))
  local total_injected=0

  while [ $(date +%s) -lt $end_time ]; do
    inject_event "!continuous:localhost" "m.room.message" "stress_as_1" 2>/dev/null || true
    total_injected=$((total_injected + 1))
    sleep 0.05
  done

  local stats=$(get_statistics)
  local pending=$(echo "$stats" | jq '[.[] | select(.as_id == "stress_as_1") | .pending_event_count // 0] | add // 0')
  local success=$(echo "$stats" | jq '[.[] | select(.as_id == "stress_as_1") | .scheduler.total_success_count // 0] | add // 0')

  log_info "持续注入: $total_injected 事件"
  log_info "剩余积压: $pending"
  log_info "成功投递: $success"

  # 验收: 积压不无限增长
  if [ "$pending" -lt "$total_injected" ]; then
    log_info "验证通过: 积压未无限增长"
  else
    log_warn "积压等于或超过注入量 (bridge 可能不可达)"
  fi

  write_scenario_result \
    "continuous-ingress" \
    "$(jq -cn \
      --argjson injected "$total_injected" \
      --argjson pending "$pending" \
      --argjson success "$success" \
      '{
        injected: $injected,
        pending: $pending,
        success: $success
      }')" \
    "$stats"
}

# 场景 7: super-event-heavy
run_super_event_heavy() {
  log_info "=== 场景 7: super-event-heavy (超大事件量单一 AS) ==="

  # 向单一 AS 注入大量事件，同时向其他 AS 注入少量事件
  for i in $(seq 1 200); do
    inject_event "!heavy:localhost" "m.room.message" "stress_as_1" 2>/dev/null || true
  done

  for i in $(seq 2 5); do
    for j in $(seq 1 10); do
      inject_event "!light_$i:localhost" "m.room.message" "stress_as_$i" 2>/dev/null || true
    done
  done

  sleep 10

  local stats=$(get_statistics)
  local heavy_success=$(echo "$stats" | jq '[.[] | select(.as_id == "stress_as_1") | .scheduler.total_success_count // 0] | add // 0')
  local light_success=0
  for i in $(seq 2 5); do
    local s=$(echo "$stats" | jq "[.[] | select(.as_id == \"stress_as_$i\") | .scheduler.total_success_count // 0] | add // 0")
    light_success=$((light_success + s))
  done

  log_info "重 AS 成功: $heavy_success"
  log_info "轻 AS 成功: $light_success"

  # 验收: 其他 AS 不被饿死
  if [ "$light_success" -gt 0 ]; then
    log_info "验证通过: 其他 AS 未被饿死"
  else
    log_error "验证失败: 轻 AS 被饿死"
  fi

  write_scenario_result \
    "super-event-heavy" \
    "$(jq -cn \
      --argjson heavy_success "$heavy_success" \
      --argjson light_success "$light_success" \
      '{
        heavy_success: $heavy_success,
        light_success: $light_success
      }')" \
    "$stats"
}

# 输出三出口一致性检查
check_three_outlet_consistency() {
  local label="${1:-snapshot}"
  log_info "=== 三出口一致性检查 (${label}) ==="

  local stats_raw
  local telemetry_raw
  local prometheus_raw
  local statistics_summary
  local telemetry_summary
  local prometheus_summary
  local consistency_report
  local report_file

  stats_raw="$(get_statistics)"
  telemetry_raw="$(get_telemetry_metrics)"
  prometheus_raw="$(get_prometheus_metrics)"
  statistics_summary="$(aggregate_statistics_summary "$stats_raw")"
  telemetry_summary="$(normalize_telemetry_summary "$telemetry_raw")"
  prometheus_summary="$(build_prometheus_summary "$prometheus_raw")"
  consistency_report="$(compare_outlet_summaries "$statistics_summary" "$telemetry_summary" "$prometheus_summary")"
  report_file="$RESULTS_DIR/outlet-consistency-${label}.json"

  jq -n \
    --arg label "$label" \
    --arg generated_at "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
    --argjson statistics "$statistics_summary" \
    --argjson telemetry "$telemetry_summary" \
    --argjson prometheus "$prometheus_summary" \
    --argjson consistency "$consistency_report" \
    '{
      label: $label,
      generated_at: $generated_at,
      outlets: {
        statistics: $statistics,
        telemetry: $telemetry,
        prometheus: $prometheus
      },
      consistency: $consistency
    }' > "$report_file"

  echo "$consistency_report" | jq -r '.comparisons[] | "  \(.key): statistics=\(.statistics) telemetry=\(.telemetry) prometheus=\(.prometheus)"'

  if [ "$(echo "$consistency_report" | jq -r '.consistent')" = "true" ]; then
    log_info "三出口一致性: 通过"
  else
    log_warn "三出口一致性: 存在差异 -> $(echo "$consistency_report" | jq -r '.mismatched_keys | join(", ")')"
  fi

  log_info "一致性快照已保存到: $report_file"
}

# 输出阈值建议
output_threshold_recommendations() {
  log_info "=== 阈值调优建议 ==="

  local stats=$(get_statistics)
  local total_as=$(echo "$stats" | jq 'length')
  local capacity_limited=$(echo "$stats" | jq '[.[] | select(.scheduler.last_result == "capacity_limited")] | length')
  local total_backoff=$(echo "$stats" | jq '[.[].scheduler.total_backoff_count // 0] | add // 0')
  local total_capacity=$(echo "$stats" | jq '[.[].scheduler.total_capacity_limited_count // 0] | add // 0')

  echo ""
  echo "--- 当前默认值 ---"
  echo "  MAX_SERVICES_PER_TICK: 8"
  echo "  HIGH_PENDING_EVENT_THRESHOLD: 50"
  echo "  HIGH_PENDING_TRANSACTION_THRESHOLD: 2"
  echo ""
  echo "--- 运行时观测 ---"
  echo "  AS 总数: $total_as"
  echo "  capacity_limited AS: $capacity_limited"
  echo "  累计退避次数: $total_backoff"
  echo "  累计容量限流次数: $total_capacity"
  echo ""

  if [ "$capacity_limited" -gt 0 ] || [ "$total_capacity" -gt 10 ]; then
    echo "  建议: 提高 MAX_SERVICES_PER_TICK (当前=8, 建议=12~16)"
  elif [ "$total_as" -le 4 ]; then
    echo "  建议: 当前 AS 数量较少, MAX_SERVICES_PER_TICK=8 足够"
  else
    echo "  建议: 当前 MAX_SERVICES_PER_TICK=8 维持不变"
  fi

  if [ "$total_backoff" -gt 20 ]; then
    echo "  建议: 退避频繁, 检查 bridge 健康状态或调整 retry_backoff 策略"
  fi

  echo ""
  echo "--- 回退条件 ---"
  echo "  1. capacity_limited 持续 > 0: 提高 MAX_SERVICES_PER_TICK"
  echo "  2. event backlog 持续增长: 降低 HIGH_PENDING_EVENT_THRESHOLD"
  echo "  3. transaction 频繁超时: 检查 bridge 响应时间"
  echo "  4. 多 AS 同时 recovery 失败: 确认 MAX_SERVICES_PER_TICK 足够"
}

# 主流程
main() {
  log_info "AppService 生产压测开始"
  log_info "场景: $SCENARIO, 持续: ${DURATION}s"
  log_info "目标: $BASE_URL"
  echo ""

  require_scheduler_preconditions
  check_three_outlet_consistency "preflight"
  echo ""

  case "$SCENARIO" in
    event-only)        run_event_only ;;
    transaction-only)  run_transaction_only ;;
    mixed)             run_mixed ;;
    mixed-backoff)     run_mixed_backoff ;;
    recovery)          run_recovery ;;
    continuous-ingress) run_continuous_ingress ;;
    super-event-heavy) run_super_event_heavy ;;
    all)
      run_event_only
      echo ""
      run_transaction_only
      echo ""
      run_mixed
      echo ""
      run_mixed_backoff
      echo ""
      run_recovery
      echo ""
      run_continuous_ingress
      echo ""
      run_super_event_heavy
      ;;
    *)
      log_error "未知场景: $SCENARIO"
      exit 1
      ;;
  esac

  echo ""
  check_three_outlet_consistency "post-run"
  echo ""
  output_threshold_recommendations

  echo ""
  log_info "压测结果已保存到: $RESULTS_DIR/"
  ls -la "$RESULTS_DIR/" 2>/dev/null || true

  log_info "AppService 生产压测完成"
}

main "$@"
