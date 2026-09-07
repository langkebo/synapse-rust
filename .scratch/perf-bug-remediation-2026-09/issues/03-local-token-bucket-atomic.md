# 03: 本地限流令牌桶改为原子读改写

**What to build:** Redis 不可用时，限流会降级到进程内的本地令牌桶。当前实现是「读状态 → 计算 → 写回状态」三步分离操作，中间没有原子性保证。多线程 runtime 下，同一 IP 的并发请求可以全部读到「令牌充足」并同时放行，使突发容量被放大到接近并发数——而这恰恰发生在后端已经不健康、最需要限流生效的时刻。本票把整个读改写序列改成单次原子操作，让本地降级路径也能严格兑现配置的突发上限。

**Blocked by:** None (can start immediately)

**Status:** done — implemented by commit `87ef15c0` on 2026-09-01

**审计条目：** #3（🟠 High）— 本地限流 token bucket 的 TOCTOU 竞态

## 验收（2026-09-07）

**实现（commit `87ef15c0` 2026-09-01）**：
- `87ef15c0`: `fix(cache): atomize local rate-limit token bucket with moka and_compute_with`
- `synapse-cache/src/lib.rs:1824`：`rate_limit_local.entry_by_ref(key).and_compute_with(|existing| {...})`
- `and_compute_with` 用 key 级锁把同一 key 的 compute 串行化，读改写原子化
- 多线程 Tokio runtime 并发请求不再全部读到 tokens=1.0 放行
- `cargo test -p synapse-cache --lib` → 110 passed, 0 failed

- [ ] 令牌桶的「读状态 → 补充 → 判定 → 扣减 → 写回」在单次原子操作内完成
- [ ] 并发单测可验证：N 个线程同时对同一 key 请求，放行总数不超过配置的突发上限（回归前会显著超出）
- [ ] 不同 key 之间的桶仍然相互独立（现有行为不回退）
- [ ] 桶的容量与 TTL 上限保持有界，不因改实现而退化为无界结构
- [ ] `cargo test -p synapse-cache` 全绿，`cargo clippy -p synapse-cache -- -D warnings` 无告警
