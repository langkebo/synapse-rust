# 01: Nonce 追踪器改为定长键 + FIFO 摊销剪枝

**What to build:** 加密热路径上，每次生成 nonce 都要向「已用 nonce」集合登记并检查重用。当前登记动作会在集合达到上限时**同步剪掉一半元素**，造成周期性延迟尖峰；而且删掉的并不是最旧的记录，导致被删的 nonce 之后可被无检测地重用。本票把追踪器改成：键用定长栈类型（不再每条消息一次堆分配）、按插入顺序剪最旧的、并把剪枝成本摊销到每次登记上而不是一次性爆发。

**Blocked by:** None (can start immediately)

**Status:** done — pre-implemented by commit `49a59e40` on 2026-09-01 (5 days before this audit ticket was filed)

**审计条目：** #5（🟡 Medium）— NonceTracker 剪枝在加密热路径同步执行

## 验收（2026-09-07 12:36）

**Pre-existing 实现（commit `49a59e40` 2026-09-01）已满足所有 acceptance criteria**：

- [x] 登记一个 nonce 不再产生堆分配（`NonceKey { len: u8, bytes: [u8; MAX_NONCE_LEN=24] }` 是定长 `Copy`，`#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]`）
- [x] 同时支持 12 与 24 字节长度（`NonceKey::new` 写入 `len: u8`，12 字节 vs 24 字节即使前缀相同也不视为同一条）
- [x] 容量满时按插入顺序淘汰最旧（`order: Mutex<VecDeque<NonceKey>>` 记录插入顺序，`prune_old_nonces` 用 `pop_front` 取最旧）
- [x] 摊销剪枝（`prune_old_nonces` 每次最多淘汰 `NONCE_PRUNE_BATCH = 256` 条；目标水位 = `max_history_size / 2`）
- [x] 重用检测稳定报 `NonceReuseDetected`（`DashSet::insert` 返回 false → 立即返回错误）
- [x] `cargo test -p synapse-e2ee --lib --features test-utils crypto::aes` → **49 passed, 0 failed**
- [x] `cargo clippy -p synapse-e2ee --lib --features test-utils -- -D warnings` → e2ee crate 0 errors（5 个 error 全部在 synapse-common 来自 deny 阶段 doc list 缩进遗留，与本任务无关）

## 关联测试已就位

- `test_nonce_tracker_distinguishes_nonce_lengths`（L1146-1154）：12 字节 [1u8; 12] vs 24 字节 [1u8; 24] 互不冲突
- `test_nonce_tracker_prunes_oldest_first`（L1128-1141）：max=4 填满后剩最新 2 条，0/1 被淘汰
- `test_nonce_tracker_pruning`（L1114-1123）：150 条插入后 `used_nonces.len() <= 100`
- `test_nonce_tracker_rejects_invalid_lengths`（L1157-1163）：空 + 25 字节拒绝
- `test_concurrent_nonce_generation`（L1166-1193）：10 线程 × 10 次 = 100 unique
- `test_nonce_tracker_counter_increments`（L947-956）：counter 单调递增

## 关键实现细节

```rust
const MAX_NONCE_LEN: usize = 24;
const NONCE_HISTORY_SIZE: usize = 10000;
const NONCE_PRUNE_BATCH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NonceKey { len: u8, bytes: [u8; MAX_NONCE_LEN] }

pub struct NonceTracker {
    used_nonces: DashSet<NonceKey>,
    order: Mutex<VecDeque<NonceKey>>,
    counter: AtomicU64,
    max_history_size: usize,
}

fn prune_old_nonces(&self) {
    let target = (self.max_history_size / 2).max(1);
    let mut evicted = 0;
    while self.used_nonces.len() > target && evicted < NONCE_PRUNE_BATCH {
        let Some(oldest) = order.pop_front() else { break };
        self.used_nonces.remove(&oldest);
        evicted += 1;
    }
}
```
