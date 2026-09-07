# 05: 限流中间件去掉每请求的配置深拷贝

**What to build:** 限流中间件是全体 HTTP 请求的必经路径，当前在处理每个请求时都深拷贝一整份限流配置（含多个字符串列表与映射字段），等于每个请求付出多次堆分配与全量字符串复制。配置在启动时加载、运行期只读，没有任何理由复制。本票改为借用，消灭这份纯属浪费的每请求开销。

**Blocked by:** None (can start immediately)

**Status:** done — implemented by commit `b10f34b3` on 2026-09-01

**审计条目：** #6（🟡 Medium）— 限流中间件每请求深拷贝整个配置

## 验收（2026-09-07）

**实现（commit `b10f34b3` 2026-09-01）**：
- `b10f34b3`: `perf(services): batch Redis deletes & burn-after-read processing, rate-limit config by reference`
- `src/web/middleware/rate_limit.rs`：RateLimitConfig 从 clone 改为 `&RateLimitConfig` 借用
- 每请求不再深拷贝配置（消除多个字符串列表与映射字段的每请求堆分配）

- [ ] 中间件不再克隆限流配置，改为借用（可验证：相关克隆调用消失）
- [ ] 启用状态、豁免路径、IP 头优先级、失败开放等行为判定与改动前完全一致
- [ ] 无生命周期冲突导致的编译回退（不得靠再引入一次克隆来“解决”借用问题）
- [ ] `cargo check` 通过、`cargo clippy -- -D warnings` 无告警
- [ ] 限流相关的既有测试全绿
