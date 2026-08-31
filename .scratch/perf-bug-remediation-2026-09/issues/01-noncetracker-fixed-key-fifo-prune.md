# 01: Nonce 追踪器改为定长键 + FIFO 摊销剪枝

**What to build:** 加密热路径上，每次生成 nonce 都要向「已用 nonce」集合登记并检查重用。当前登记动作会在集合达到上限时**同步剪掉一半元素**，造成周期性延迟尖峰；而且删掉的并不是最旧的记录，导致被删的 nonce 之后可被无检测地重用。本票把追踪器改成：键用定长栈类型（不再每条消息一次堆分配）、按插入顺序剪最旧的、并把剪枝成本摊销到每次登记上而不是一次性爆发。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

**审计条目：** #5（🟡 Medium）— NonceTracker 剪枝在加密热路径同步执行

- [ ] 登记一个 nonce 不再产生堆分配（键为 `Copy` 的定长类型，不再用 `Vec<u8>`）
- [ ] 追踪器同时支持 12 字节与 24 字节两种 nonce 长度，且不发生长度歧义（不同长度不视为同一 nonce）
- [ ] 达到容量上限时按**插入顺序**淘汰最旧的条目，淘汰行为可被单测验证
- [ ] 淘汰成本摊销：每次登记最多淘汰一个固定小批量，不再出现「一次登记触发数千次删除」的尖峰
- [ ] 同进程内重复 nonce 仍然稳定报 `NonceReuseDetected`（行为不回退）
- [ ] `cargo test -p synapse-e2ee` 全绿，`cargo clippy -p synapse-e2ee -- -D warnings` 无告警
