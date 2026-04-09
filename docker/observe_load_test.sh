#!/usr/bin/env bash
set -euo pipefail

container="${1:-}"
interval_s="${2:-1}"
out_path="${3:-load_test_observe.tsv}"

if [[ -z "$container" ]]; then
  echo "usage: $0 <container_name_or_id> [interval_s] [out_path]" >&2
  exit 2
fi

ts() { date +%s; }

docker_stats_line() {
  docker stats --no-stream --format '{{.CPUPerc}}\t{{.MemUsage}}\t{{.NetIO}}\t{{.BlockIO}}\t{{.PIDs}}' "$container" 2>/dev/null || true
}

ns_exec() {
  docker run --rm --pid="container:$container" --net="container:$container" alpine:3.20 /bin/sh -lc "$1" 2>/dev/null || true
}

container_main_pid() {
  ns_exec '
for p in /proc/[0-9]*/comm; do
  pid="${p#/proc/}"
  pid="${pid%/comm}"
  comm="$(cat "$p" 2>/dev/null || true)"
  if [ "$comm" = "synapse-rust" ] || [ "$comm" = "synapse_worker" ] || [ "$comm" = "synapse-worker" ]; then
    echo "$pid"
    exit 0
  fi
done

best=""
best_c="-1"
for d in /proc/[0-9]*/fd; do
  pid="${d#/proc/}"
  pid="${pid%/fd}"
  c="$(ls -1 "$d" 2>/dev/null | wc -l || true)"
  if [ -n "$c" ] && [ "$c" -gt "$best_c" ]; then
    best_c="$c"
    best="$pid"
  fi
done
echo "${best:-1}"
'
}

container_fd_count() {
  local pid="${1:-}"
  if [[ -z "$pid" ]]; then
    pid="$(container_main_pid)"
  fi
  ns_exec "ls -1 /proc/${pid}/fd 2>/dev/null | wc -l"
}

container_tcp_sockstat() {
  ns_exec 'cat /proc/net/sockstat 2>/dev/null | tr "\n" " "'
}

sockstat_field() {
  local key="$1"
  local line="$2"
  echo "$line" | awk -v k="$key" '{
    for (i = 1; i <= NF; i++) {
      if ($i == k && i + 1 <= NF) { print $(i + 1); exit }
    }
  }'
}

softirq_sum() {
  local name="$1"
  ns_exec "awk '\$1==\"$name:\" {s=0; for(i=2;i<=NF;i++) s+=\$i; print s}' /proc/softirqs 2>/dev/null"
}

tcp_ext_counter() {
  local key="$1"
  ns_exec "awk '
    \$1==\"TcpExt:\" {if (nr==0){for(i=2;i<=NF;i++) k[i]=\$i; nr=1; next}
                     if (nr==1){for(i=2;i<=NF;i++) v[k[i]]=\$i; print v[\"$key\"]; exit}}
  ' /proc/net/netstat 2>/dev/null"
}

tcp_snmp_counter() {
  local key="$1"
  ns_exec "awk '
    \$1==\"Tcp:\" {if (nr==0){for(i=2;i<=NF;i++) k[i]=\$i; nr=1; next}
                  if (nr==1){for(i=2;i<=NF;i++) v[k[i]]=\$i; print v[\"$key\"]; exit}}
  ' /proc/net/snmp 2>/dev/null"
}

printf "ts\tcpu\tmem\tnet_io\tblock_io\tpids\tmain_pid\tfd_count\ttcp_inuse\ttcp_tw\ttcp_alloc\ttcp_mem\tlisten_overflows\tlisten_drops\ttcp_retrans_segs\tsoftirq_net_rx\tsoftirq_net_tx\n" >"$out_path"

while true; do
  now="$(ts)"
  stats="$(docker_stats_line)"

  cpu=""
  mem=""
  net_io=""
  block_io=""
  pids=""
  if [[ -n "$stats" ]]; then
    cpu="$(echo "$stats" | awk -F'\t' '{print $1}')"
    mem="$(echo "$stats" | awk -F'\t' '{print $2}')"
    net_io="$(echo "$stats" | awk -F'\t' '{print $3}')"
    block_io="$(echo "$stats" | awk -F'\t' '{print $4}')"
    pids="$(echo "$stats" | awk -F'\t' '{print $5}')"
  fi

  main_pid="$(container_main_pid)"
  fd_count="$(container_fd_count "$main_pid")"
  sockstat="$(container_tcp_sockstat)"
  tcp_inuse="$(sockstat_field inuse "$sockstat")"
  tcp_tw="$(sockstat_field tw "$sockstat")"
  tcp_alloc="$(sockstat_field alloc "$sockstat")"
  tcp_mem="$(sockstat_field mem "$sockstat")"
  listen_overflows="$(tcp_ext_counter ListenOverflows)"
  listen_drops="$(tcp_ext_counter ListenDrops)"
  tcp_retrans_segs="$(tcp_snmp_counter RetransSegs)"
  net_rx="$(softirq_sum NET_RX)"
  net_tx="$(softirq_sum NET_TX)"

  printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" \
    "$now" "$cpu" "$mem" "$net_io" "$block_io" "$pids" "$main_pid" "$fd_count" \
    "$tcp_inuse" "$tcp_tw" "$tcp_alloc" "$tcp_mem" "$listen_overflows" "$listen_drops" "$tcp_retrans_segs" "$net_rx" "$net_tx" \
    >>"$out_path"

  sleep "$interval_s"
done
