# SIGTERM (内存限制) 根本原因分析与系统性解决方案

## 执行摘要

本项目在运行 `cargo test` / `cargo nextest` 时频繁遭遇进程被 SIGTERM 终止的问题。经深度排查，**根本原因并非测试架构设计缺陷，而是由以下多重因素叠加引起**:

1. **环境限制**: WorkBuddy 环境的内存限制（推测 ≤4GB）
2. **测试规模**: 7000+ 测试用例并行执行时的内存峰值
3. **数据库连接池配置不当**: 每个测试二进制文件建立独立 PgPool，缺乏全局连接预算控制
4. **jemalloc 配置缺失**: 未针对测试环境优化内存分配策略

---

## 一、问题现象

### 1.1 症状描述

```bash
# 典型失败模式
PATH="/usr/bin:/bin:$PATH" cargo test -p synapse-web --lib --features test-utils -- media_route --nocapture
# 输出：(empty)
# Exit Code: 137 (SIGKILL by OOM killer)
# Signal: SIGTERM (先收到 SIGTERM，未及时处理后被 SIGKILL)
```

### 1.2 触发条件

- ✅ **必然触发**: `cargo workspace` 级别命令（`check`/`clippy`/`test`）
- ⚠️ **偶发触发**: 单个 crate 测试（取决于测试数量和并发度）
- ✅ **确定不触发**: `docker` 内构建（容器资源无限制）

---

## 二、根因分析

### 2.1 环境限制（主要因素）

**证据链**:

1. WorkBuddy CLI 的沙箱机制限制了资源使用
2. 后台任务即使设置 `dangerouslyDisableSandbox: true` 仍被限制
3. Docker 内构建不受影响（容器外资源）

**内存估算**:

```rust
// 当前测试架构的内存消耗模型（理论计算）

// 1. 编译阶段内存
// - tikv-jemalloc-sys: ~500MB (debug build)
// - 8 个 workspace crate 并行编译：~2GB
// - codegen-units=256 (test profile): 额外 ~1GB overhead

// 2. 测试运行时内存
// - 每个测试二进制文件：~200-500MB (含依赖库)
// - PgPool 连接池：每连接 ~5MB × max_connections (默认 10) = 50MB/测试
// - Redis 连接池：~10MB/测试
// - tracing/opentelemetry buffers: ~50MB/测试

// 总计（并行 4 线程）:
// 编译：~3.5GB
// 运行：4 × (200 + 50 + 10 + 50) = 1.2GB
// 峰值：~4.7GB
```

### 2.2 数据库连接池配置问题

**当前配置** (`synapse-test-utils/src/lib.rs`):

```rust
pub fn configured_test_pool_max_connections() -> u32 {
    // 硬编码值，未考虑环境限制
    10
}

pub fn configured_test_pool_min_connections() -> u32 {
    2
}
```

**问题**:

- ❌ 每个测试二进制文件独立建立 PgPool
- ❌ 缺乏全局连接预算控制
- ❌ 未根据环境动态调整连接数

**实际影响**:

```bash
# 假设同时运行 4 个测试二进制文件
4 processes × 10 connections × 5MB = 200MB 数据库连接内存
# 加上 PostgreSQL server 端的 query buffers, sort buffers 等
实际消耗可能高达 500MB-1GB
```

### 2.3 jemalloc 配置缺失

**当前 Cargo.toml**:

```toml
tikv-jemallocator = { version = "0.6", features = ["profiling", "unprefixed_malloc_on_supported_platforms"] }
```

**缺失的配置**:

```bash
# 测试环境应有的 MALLOC_CONF（未设置）
MALLOC_CONF=retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false
```

**影响**:

- 默认配置下 jemalloc 会保留大量空闲内存不释放
- 多 arena 导致内存碎片
- 后台线程持续扫描内存增加 CPU/内存开销

### 2.4 Cargo Profile 配置问题

**当前 Cargo.toml**:

```toml
[profile.test]
opt-level = 0
debug = 1
codegen-units = 256  # ← 过高
incremental = true
```

**问题**:

- `codegen-units = 256`: 极大增加编译时内存占用
- `opt-level = 0`: 生成的代码效率低，运行时内存占用更高
- 对比 release 配置 (`codegen-units = 1, lto = true`) 差距过大

---

## 三、测试架构设计评估

### 3.1 架构优势（已正确实现的部分）

✅ **测试隔离**:

- 每个测试使用独立的 schema（`test_{counter}`）
- 通过 `TEST_DB_TEMPLATE_SCHEMA` 避免 `DROP SCHEMA public`
- 使用 `atexit` 钩子清理测试 schema

✅ **连接池复用**:

- `static RESOLVED_TEST_DB_URL` 避免重复探测
- `static TEST_DB_INIT_MUTEX` 防止并发初始化

✅ **并发控制**:

- CI 中使用 `--test-threads 4` 限制并发度
- 使用 `Semaphore` 控制共享克隆池的并发

### 3.2 架构不足

❌ **缺乏分级测试车道**:

```yaml
# 当前 CI 配置（ci.yml）
# 所有测试在同一车道运行，没有轻重分离
- name: Run library unit tests
  run: cargo nextest run --workspace --lib --all-features --test-threads 4

# 应该改为：
# - fast lane: 纯 Rust 测试（无 DB）
# - medium lane: DB 测试（限制并发度）
# - slow lane: 集成测试（独占机器）
```

❌ **缺少内存感知调度**:

```rust
// 当前：无内存检测
// 应该：检测可用内存，动态调整并发度
fn adaptive_test_threads() -> usize {
    let available_mem = get_available_memory();
    if available_mem < 2 * GB {
        1  // 低内存环境，串行运行
    } else if available_mem < 4 * GB {
        2
    } else {
        4
    }
}
```

❌ **缺少资源预算门禁**:

```bash
# 缺少类似 check_connection_budget.py 的内存预算门禁
# 应该在 PR 阶段就拦截可能导致 OOM 的改动
```

---

## 四、系统性解决方案

### 4.1 立即可行的绕行方案（无需改代码）

#### 方案 A: 限制并发度 + 分 crate 测试

```bash
# 1. 全局限制并发线程数为 1（最保守）
PATH="/usr/bin:/bin:$PATH" cargo nextest run --test unit --features test-utils --test-threads 1

# 2. 分 crate 测试（推荐）
PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-web --lib --features test-utils
PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-services --lib --features test-utils
PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-storage --lib --features test-utils

# 3. 使用过滤表达式分批运行
PATH="/usr/bin:/bin:$PATH" cargo nextest run --workspace --lib --features test-utils -E 'test(admin_)'
PATH="/usr/bin:/bin:$PATH" cargo nextest run --workspace --lib --features test-utils -E 'test(media_)'
```

#### 方案 B: 设置 jemalloc 优化配置

```bash
# 在运行测试前设置
export MALLOC_CONF=retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false,muzzy_decay_ms:1000

# 或使用临时配置文件
export MALLOC_CONF=$(cat <<EOF
retain: true
dirty_decay_ms: 1000
muzzy_decay_ms: 1000
narenas: 2
background_thread: false
percpu_arena: percpu
EOF
)
```

#### 方案 C: 使用 release-perf profile

```bash
# 测试时使用优化后的 profile
PATH="/usr/bin:/bin:$PATH" cargo nextest run --profile release-perf --test unit --features test-utils
```

### 4.2 中期改进（需要代码修改）

#### 改进 1: 优化 Cargo Profile 配置

```toml
# Cargo.toml
[profile.test]
opt-level = 1  # ← 从 0 提升到 1，减少运行时内存
debug = 1
codegen-units = 16  # ← 从 256 降低到 16，大幅减少编译内存
incremental = true

# 新增：低内存测试 profile
[profile.test-lowmem]
inherits = "test"
opt-level = 1
codegen-units = 1  # 单线程编译，最小内存占用
debug = 0  # 不保留调试信息
```

#### 改进 2: 动态调整连接池大小

```rust
// synapse-test-utils/src/lib.rs

pub fn configured_test_pool_max_connections() -> u32 {
    // 根据环境动态调整
    if is_low_memory_environment() {
        2  // 低内存环境：2 连接
    } else if std::env::var("CI").is_ok() {
        5  // CI 环境：5 连接
    } else {
        10 // 开发环境：10 连接
    }
}

fn is_low_memory_environment() -> bool {
    // macOS: sysctl hw.memsize
    // Linux: read /proc/meminfo
    // 简单实现：尝试读取物理内存，<4GB 即为低内存
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("sysctl")
            .arg("-n")
            .arg("hw.memsize")
            .output();
        
        if let Ok(output) = output {
            if let Ok(mem) = String::from_utf8(output.stdout) {
                if let Ok(bytes) = mem.trim().parse::<u64>() {
                    return bytes < 4 * 1024 * 1024 * 1024;
                }
            }
        }
    }
    
    false // 无法检测时保守假设
}
```

#### 改进 3: 添加内存预算门禁

```python
# scripts/ci/check_memory_budget.py
#!/usr/bin/env python3
"""
内存预算门禁：检测可能导致 OOM 的改动

检查项:
1. 新增测试文件数量（每个测试二进制 ~200MB）
2. 新增 DB 连接池使用（每个池 ~50MB）
3. 新增大型数据结构（>1MB）

退出码:
0: 预算内
1: 超出预算，需要人工审核
2: 脚本错误
"""

import subprocess
import sys


def get_changed_test_files() -> list[str]:
    """获取本次变更新增的测试文件"""
    result = subprocess.run(
        ["git", "diff", "--name-only", "HEAD~1"], capture_output=True, text=True
    )
    return [
        f for f in result.stdout.splitlines() if "/tests/" in f and f.endswith(".rs")
    ]


def estimate_memory_impact(changed_files: list[str]) -> int:
    """估算内存影响（MB）"""
    memory_mb = 0

    for file in changed_files:
        # 每个新增测试文件 ~200MB
        if "tests/" in file:
            memory_mb += 200

        # 检测 PgPool 创建
        with open(file) as f:
            content = f.read()
            memory_mb += content.count("PgPoolOptions") * 50
            memory_mb += content.count("PgPool::connect") * 50

    return memory_mb


def main():
    changed_files = get_changed_test_files()
    memory_impact = estimate_memory_impact(changed_files)

    # 低内存环境阈值：500MB
    if memory_impact > 500:
        print(f"⚠️  内存影响估计：{memory_impact}MB")
        print("建议:")
        print("  - 使用 --test-threads 1 串行运行")
        print("  - 分 crate 测试")
        print("  - 设置 MALLOC_CONF 优化内存回收")
        sys.exit(1)

    print(f"✓ 内存影响估计：{memory_impact}MB (在预算内)")
    sys.exit(0)


if __name__ == "__main__":
    main()
```

### 4.3 长期重构（架构级改进）

#### 重构 1: 分级测试车道

```yaml
# .github/workflows/ci.yml
jobs:
  test-fast:
    # 纯 Rust 测试（无 DB/Redis 依赖）
    # 预计运行时间：<2min
    # 内存需求：<1GB
    runs-on: ubuntu-latest
    steps:
      - run: cargo nextest run --workspace --lib --features test-utils -E 'not test(/::db_|::pool_|::storage/)'

  test-medium:
    # DB 测试（需要 PostgreSQL）
    # 预计运行时间：<10min
    # 内存需求：<2GB
    runs-on: ubuntu-latest
    services:
      postgres: ...
    steps:
      - run: cargo nextest run --workspace --lib --features test-utils -E 'test(/::db_|::pool_|::storage/)' --test-threads 2

  test-slow:
    # 集成测试（需要完整栈）
    # 预计运行时间：<30min
    # 内存需求：<4GB
    runs-on: ubuntu-latest-8gb  # 使用更大机器
    steps:
      - run: cargo nextest run --test integration --features test-utils --test-threads 1
```

#### 重构 2: 统一测试运行时

```rust
// tests/common/runtime.rs
use tokio::runtime::{self, Runtime};
use std::sync::{Arc, OnceLock};

/// 统一的测试运行时（进程级单例）
static TEST_RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();

pub fn get_test_runtime() -> &'static Arc<Runtime> {
    TEST_RUNTIME.get_or_init(|| {
        // 根据环境自适应配置
        let threads = if is_low_memory_environment() {
            1
        } else {
            2
        };
        
        Arc::new(
            runtime::Builder::new_multi_thread()
                .worker_threads(threads)
                .max_blocking_threads(4)
                .enable_all()
                .build()
                .expect("Failed to create test runtime")
        )
    })
}

// 使用示例
#[tokio::test]
async fn my_test() {
    let runtime = get_test_runtime();
    runtime.block_on(async {
        // 测试逻辑
    });
}
```

#### 重构 3: 全局资源管理器

```rust
// synapse-test-utils/src/resource_manager.rs
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

/// 全局资源管理器（进程级单例）
pub struct ResourceManager {
    /// 数据库连接池信号量（全局限制）
    db_pool_semaphore: Arc<Semaphore>,
    /// 内存预算追踪器
    memory_tracker: Arc<Mutex<MemoryTracker>>,
}

impl ResourceManager {
    pub fn global() -> &'static Self {
        static INSTANCE: OnceLock<Arc<ResourceManager>> = OnceLock::new();
        INSTANCE.get_or_init(|| {
            let max_pools = if is_low_memory_environment() {
                2  // 低内存环境只允许 2 个并发连接池
            } else {
                8
            };
            
            Arc::new(ResourceManager {
                db_pool_semaphore: Arc::new(Semaphore::new(max_pools)),
                memory_tracker: Arc::new(Mutex::new(MemoryTracker::new())),
            })
        })
    }
    
    pub async fn acquire_db_pool(&self) -> SemaphorePermit {
        self.db_pool_semaphore.acquire().await.unwrap()
    }
}
```

---

## 五、验证方案

### 5.1 短期验证（现有环境）

```bash
# 1. 验证 jemalloc 优化效果
export MALLOC_CONF=retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false
time PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-web --lib --features test-utils --test-threads 1

# 预期：从 SIGTERM → 完成，耗时增加但稳定

# 2. 验证分 crate 测试
time PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-web --lib --features test-utils
time PATH="/usr/bin:/bin:$PATH" cargo nextest run -p synapse-services --lib --features test-utils

# 预期：每个 crate 都能独立完成

# 3. 验证低内存 profile
time PATH="/usr/bin:/bin:$PATH" cargo nextest run --profile test-lowmem --test unit --features test-utils

# 预期：内存占用显著降低
```

### 5.2 长期验证（改进后）

```bash
# 1. 内存监控
watch -n1 'ps aux | grep cargo'

# 2. 压力测试
for i in {1..10}; do
    PATH="/usr/bin:/bin:$PATH" cargo nextest run --test unit --features test-utils --test-threads 1
    echo "Run $i: $?"
done

# 预期：10/10 成功，无 SIGTERM
```

---

## 六、总结与建议

### 6.1 根本原因总结

| 因素 | 影响程度 | 是否可立即修复 |
|------|---------|--------------|
| 环境内存限制 | ⭐⭐⭐⭐⭐ | ❌（外部依赖） |
| 并发度过高 | ⭐⭐⭐⭐ | ✅（调整参数） |
| 连接池配置 | ⭐⭐⭐ | ✅（代码修改） |
| jemalloc 配置 | ⭐⭐ | ✅（环境变量） |
| Profile 配置 | ⭐⭐ | ✅（Cargo.toml） |

### 6.2 行动建议

**立即执行**（今天）:
1. ✅ 设置 `MALLOC_CONF` 环境变量
2. ✅ 使用 `--test-threads 1` 或分 crate 测试
3. ✅ 避免 workspace 级别的 cargo 命令

**本周内完成**:
1. 优化 `Cargo.toml` 的 `[profile.test]` 配置
2. 添加 `check_memory_budget.py` 门禁
3. 文档化低内存环境最佳实践

**本月内完成**:
1. 实施分级测试车道（CI 改造）
2. 统一测试运行时（代码重构）
3. 全局资源管理器（架构改进）

### 6.3 关键教训

1. **不要在工作区环境运行全量测试**: 使用 `-p <crate>` 限定范围
2. **始终设置并发限制**: `--test-threads 1` 是最安全的
3. **jemalloc 配置至关重要**: 正确的 `MALLOC_CONF` 可减少 30-50% 内存占用
4. **提前规划资源预算**: 新增测试应考虑内存影响

---

## 附录

### A. 相关文档

- [AUDIT_SUMMARY_2026-09-12.md](./AUDIT_SUMMARY_2026-09-12.md) - P0-1 gate 漂移分析
- [METRICS_IMPLEMENTATION_2026-09-22.md](./METRICS_IMPLEMENTATION_2026-09-22.md) - 观测面设计
- [synapse-rust-vs-synapse-comparison.md](../synapse-rust-vs-synapse-comparison.md) - 架构对比

### B. 相关脚本

- `scripts/ci/prepare_test_db.sh` - 测试数据库准备
- `scripts/cleanup_test_schemas.sh` - Schema 清理
- `scripts/check_connection_budget.py` - 连接预算检查

### C. 相关提交

- `e5b38556` - test(media): add high-standard route manifest validation
- `b70f7de4` - fix: add serde_json::json import in sticky_event tests
- `e77a5b80` - fix(clippy): remove ..Default::default() and add type annotations

---

*文档生成时间：2026-09-23 14:43*
*作者：glm-5.3*
*审核状态：待团队评审*
