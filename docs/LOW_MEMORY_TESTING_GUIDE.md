# 低内存环境测试最佳实践

## 概述

本文档提供在 WorkBuddy CLI 环境（内存限制 ≤4GB）以及其他低内存环境下运行测试的最佳实践。

## 问题背景

在 WorkBuddy CLI 环境中运行 `cargo test` 或 `cargo nextest` 时，经常遭遇进程被 SIGTERM/SIGKILL 终止的问题。根本原因包括：

1. **环境内存限制**: WorkBuddy 沙箱环境的内存限制通常 ≤4GB
2. **测试规模**: 7000+ 测试用例并行执行时的内存峰值
3. **数据库连接池配置**: 每个测试二进制文件建立独立 PgPool
4. **jemalloc 配置缺失**: 未针对测试环境优化内存分配策略

## 立即解决方案

### 方案 A: 串行运行（最保守）

```bash
# 设置 jemalloc 优化配置
export MALLOC_CONF="retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false,muzzy_decay_ms:1000"

# 串行运行单个 crate 的测试
PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-web --lib --features test-utils --test-threads 1
```

### 方案 B: 分 crate 测试（推荐）

```bash
export MALLOC_CONF="retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false"

# 分别运行各个 crate 的测试
PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-web --lib --features test-utils
PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-services --lib --features test-utils
PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-storage --lib --features test-utils
```

### 方案 C: 使用低内存 Profile

```bash
export MALLOC_CONF="retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false"

# 使用 test-lowmem profile（单 codegen-unit，禁用 debug 信息）
PATH="/usr/bin:/bin:$PATH" cargo nextest run --profile test-lowmem --test unit --features test-utils
```

### 方案 D: 过滤特定测试组

```bash
export MALLOC_CONF="retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false"

# 只运行特定类型的测试
PATH="/usr/bin:/bin:$PATH" cargo nextest run --test unit --features test-utils -E 'test(admin_)'
PATH="/usr/bin:/bin:$PATH" cargo nextest run --test unit --features test-utils -E 'test(media_)'
```

## 环境变量配置

### 必需的环境变量

```bash
# jemalloc 内存管理优化
export MALLOC_CONF="retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false,muzzy_decay_ms:1000"

# 可选：调整数据库连接池大小
export TEST_DB_MAX_CONNECTIONS=20  # 默认 40，低内存环境建议 20
export TEST_DB_SHARED_CLONE_CONCURRENCY=6  # 默认 12，低内存环境建议 6

# 调试选项
export RUST_BACKTRACE=1
```

### MALLOC_CONF 参数说明

| 参数 | 说明 | 推荐值 |
|------|------|--------|
| `retain` | 保留空闲内存而非返回给 OS（减少 malloc/mmap 系统调用） | `true` |
| `dirty_decay_ms` | 脏页回收到 OS 的延迟（ms） | `1000` |
| `muzzy_decay_ms` | muzzy 页回收到 OS 的延迟（ms） | `1000` |
| `narenas` | Arena 数量（减少内存碎片） | `2` |
| `background_thread` | 后台线程回收内存 | `false` |

## Cargo Profile 配置

### 默认测试 Profile

```toml
[profile.test]
opt-level = 1
debug = 1
codegen-units = 16
incremental = true
```

### 低内存测试 Profile

```toml
[profile.test-lowmem]
inherits = "test"
opt-level = 1
codegen-units = 1  # 单线程编译，最小化内存占用
debug = 0          # 禁用调试信息，节省 ~500MB
incremental = false  # 禁用增量编译，节省缓存内存
strip = true       # 剥离符号，节省 ~100MB
```

使用方式：

```bash
cargo nextest run --profile test-lowmem ...
```

## 内存占用估算

| 场景 | 估算内存 | 推荐并发度 |
|------|---------|----------|
| 单 crate 测试 (1 线程) | 800MB - 1.5GB | `--test-threads 1` |
| 单 crate 测试 (2 线程) | 1.5GB - 2.5GB | `--test-threads 2` |
| 多 crate 测试 (4 线程) | 3GB - 5GB | ❌ 不推荐 |
| Workspace 级测试 | 5GB+ | ❌ 必然 OOM |
| 编译阶段 (debug) | 2GB - 4GB | - |

## 自动化门禁

项目已集成内存预算门禁脚本，可在 PR 阶段检测潜在的内存问题：

```bash
python3 scripts/ci/check_memory_budget.py
```

该脚本会检测：
- 新增测试文件数量
- 新增 PgPool 创建点
- 大型静态数据结构
- 并发度配置变更

## 调试技巧

### 1. 监控内存使用

```bash
# macOS
watch -n1 'ps -o rss,vsize,command -p $(pgrep cargo)'

# Linux
watch -n1 'ps -o rss,vsize,command -p $(pgrep cargo)'
```

### 2. 启用 jemalloc 统计

```bash
export MALLOC_CONF="stats_print:true"
cargo nextest run -p synapse-web --lib --features test-utils --test-threads 1
# 测试结束后会打印详细的内存统计
```

### 3. 生成 heap profile

```bash
export MALLOC_CONF="prof:true,prof_prefix:heap.prof"
cargo nextest run -p synapse-web --lib --features test-utils --test-threads 1

# 使用 jeprof 分析
jeprof --text target/debug/synapse-web heap.prof.<pid>
```

## 常见问题

### Q: 为什么不能运行 `cargo nextest run --workspace ...`？

A: workspace 级别的命令会同时编译和运行所有 crate 的测试，内存峰值远超 4GB 限制。应该：
- 分 crate 运行：`cargo nextest run -p <crate> ...`
- 或使用过滤：`cargo nextest run --test unit -E 'test(admin_)'`

### Q: 如何知道当前环境的内存限制？

A: 

```bash
# macOS
sysctl hw.memsize

# Linux
cat /proc/meminfo | grep MemTotal
```

如果总内存 < 6GB，应视为低内存环境。

### Q: 测试失败显示 "out of shared memory" 怎么办？

A: 这是 PostgreSQL 的共享内存限制，而非物理内存：

```bash
# 降低并发度
cargo nextest run --test-threads 1

# 或清理积累的 test schema
bash scripts/cleanup_test_schemas.sh --apply
```

### Q: 能否临时提高内存预算？

A: 可以，但不推荐：

```bash
export MEMORY_BUDGET_MB=8192
python3 scripts/ci/check_memory_budget.py
```

更好的做法是优化测试架构（见下文）。

## 长期优化方向

### 1. 分级测试车道

将测试分为三个车道：

- **Fast Lane**: 纯 Rust 测试（无 DB），预计 <2min，内存 <1GB
- **Medium Lane**: DB 测试，预计 <10min，内存 <2GB
- **Slow Lane**: 集成测试，预计 <30min，需要 8GB+ 机器

### 2. 统一测试运行时

使用进程级单例的测试运行时，避免重复创建 runtime：

```rust
// tests/common/runtime.rs
use tokio::runtime::{self, Runtime};
use std::sync::{Arc, OnceLock};

static TEST_RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();

pub fn get_test_runtime() -> &'static Arc<Runtime> {
    TEST_RUNTIME.get_or_init(|| {
        Arc::new(
            runtime::Builder::new_multi_thread()
                .worker_threads(if is_low_memory_environment() { 1 } else { 2 })
                .enable_all()
                .build()
                .expect("Failed to create test runtime")
        )
    })
}
```

### 3. 全局资源管理器

实现进程级的资源管理器，限制并发 PgPool 数量：

```rust
pub struct ResourceManager {
    db_pool_semaphore: Arc<Semaphore>,  // 全局限制最大并发池数
}

impl ResourceManager {
    pub fn global() -> &'static Self { /* ... */ }
    
    pub async fn acquire_db_pool(&self) -> SemaphorePermit {
        self.db_pool_semaphore.acquire().await.unwrap()
    }
}
```

## 相关文档

- [SIGTERM_ROOT_CAUSE_ANALYSIS_2026-09-23.md](../docs/audit/SIGTERM_ROOT_CAUSE_ANALYSIS_2026-09-23.md) - 完整的根因分析
- [SIGTERM_QUICK_REFERENCE.md](../docs/SIGTERM_QUICK_REFERENCE.md) - 快速参考手册
- [AUDIT_SUMMARY_2026-09-12.md](../docs/audit/AUDIT_SUMMARY_2026-09-12.md) - Gate 漂移分析

## 更新日志

- **2026-09-23**: 初始版本，包含完整的低内存环境解决方案
- 记录了从 SIGTERM 问题排查到系统性优化的全过程

---

*最后更新：2026-09-23*
*维护者：synapse-rust 团队*
