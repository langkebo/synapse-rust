# DEPENDENCY_UPGRADE_TRACKER.md

> 深层重复依赖跟踪表，记录无法通过本地 `[patch]` 解决的 SemVer 不兼容重复依赖。
> 最后更新: 2026-06-13
> 基线: `cargo tree -d --workspace` 在 `cargo update` 后

---

## 已解决的重复依赖

| 依赖 | 旧版本 | 解决方式 | 日期 |
|------|--------|----------|------|
| `base64` | v0.21.7 (via `ron` → `config`) | 禁用 `config` 的 `ron` feature，连带消除 `toml_datetime`/`toml_edit`/`winnow` 重复 | 2026-06-13 |

## 当前重复依赖（需上游升级）

### 1. hashbrown: 3 版本 (v0.14.5 / v0.15.5 / v0.17.1)

| 版本 | 引入路径 | 根本原因 |
|------|----------|----------|
| v0.14.5 | `dashmap 6.2.1` → hashbrown 0.14 | dashmap 6.x 锁定 hashbrown 0.14 |
| v0.14.5 | `hashlink 0.8.4` → `yaml-rust2` → `config 0.14.1` | config 依赖旧版 yaml 解析器 |
| v0.15.5 | `hashlink 0.10.0` → `sqlx-core 0.8.6` | sqlx 使用 hashlink 0.10 |
| v0.17.1 | `indexmap 2.14.0` → `h2`, `serde_yaml`, `sqlx-core`, `tower` | indexmap 2.x 升级到 hashbrown 0.17 |

**为什么不能本地解决**: 三个版本之间 SemVer 不兼容，API 有 breaking changes。`[patch.crates-io]` 无法覆盖版本范围约束。

**上游依赖**:
- `dashmap` 需升级到支持 `hashbrown 0.17`（track: https://github.com/xacrimon/dashmap）
- `config` 需升级 `yaml-rust2` → `hashlink 0.10+`（track: https://github.com/rust-cli/config-rs）
- 三者统一到 `hashbrown 0.17` 需所有上游同步

**检查频率**: 每月

### 2. hashlink: 2 版本 (v0.8.4 / v0.10.0)

| 版本 | 引入路径 |
|------|----------|
| v0.8.4 | `yaml-rust2` → `config 0.14.1` |
| v0.10.0 | `sqlx-core 0.8.6` |

**为什么不能本地解决**: 同 hashbrown，底层依赖版本不兼容。

**上游依赖**: `config` 升级 yaml 解析器

### 3. getrandom: 3 版本 (v0.2.17 / v0.3.4 / v0.4.2)

| 版本 | 引入路径 | 根本原因 |
|------|----------|----------|
| v0.2.17 | `rand_core 0.6.4` → `rand 0.8.5` → `sqlx`, `vodozemac`, `fake`, `rsa`, `argon2` 等 | rand 0.8 生态 |
| v0.3.4 | `rand_core 0.9.5` → `rand 0.9.4` → `opentelemetry_sdk`, `tungstenite` | rand 0.9 生态 |
| v0.4.2 | `rand 0.10.0` → `quickcheck` (dev), `tempfile`, `uuid` | rand 0.10 生态 |

**为什么不能本地解决**: 三个版本的 `rand` 生态互不兼容，分别由不同主版本的核心 crate 依赖。

**上游依赖**:
- `sqlx` / `vodozemac` / `rsa` / `argon2` 需升级到 `rand 0.9+`
- `opentelemetry` 需升级到 `rand 0.10`
- `quickcheck` 的 `rand 0.10` 仅 dev-dependency，影响较低

**检查频率**: 每月

### 4. rand / rand_core / rand_chacha: 3 版本

| 版本 | 引入路径 |
|------|----------|
| v0.8.5 | `sqlx`, `vodozemac`, `fake`, `num-bigint-dig`, `rsa`, `argon2`, `ed25519-dalek` 等 |
| v0.9.4 | `opentelemetry_sdk`, `tungstenite` |
| v0.10.0 | `quickcheck` (dev-dependency only) |

**为什么不能本地解决**: 同 getrandom 问题，rand 生态主版本不兼容。

**上游依赖**: 同 getrandom 条目

### 5. prost / prost-derive: 2 版本 (v0.13.5 / v0.14.4)

| 版本 | 引入路径 |
|------|----------|
| v0.13.5 | `vodozemac 0.9.0` |
| v0.14.4 | `opentelemetry-otlp`, `tonic-prost` |

**为什么不能本地解决**: prost 0.13 和 0.14 是 breaking change。vodozemac 和 opentelemetry 分别锁定不同主版本。

**上游依赖**:
- `vodozemac` 需升级到 `prost 0.14`（track: https://github.com/matrix-org/vodozemac）
- 或 `opentelemetry-otlp` 降级到 `prost 0.13`（不可行）

**检查频率**: 每月

### 6. itertools: 3 版本 (v0.10.5 / v0.13.0 / v0.14.0)

| 版本 | 引入路径 |
|------|----------|
| v0.10.5 | `criterion 0.5.1` (dev/bench) |
| v0.13.0 | `redis 0.27.6` |
| v0.14.0 | `prost-derive`, `rav1e` (image processing) |

**为什么不能本地解决**: 不同主版本 API 不兼容。`criterion` 为 dev 依赖，影响较低。

**上游依赖**:
- `redis` 可考虑升级到 0.29（使用 `itertools 0.14`），但需评估 breaking changes
- `criterion` 可升级到 0.6，但 API 有 breaking changes

**检查频率**: 每季度

### 7. socket2: 2 版本 (v0.5.10 / v0.6.4)

| 版本 | 引入路径 |
|------|----------|
| v0.5.10 | `redis 0.27.6` |
| v0.6.4 | `dns-lookup`, `hyper-util`, `tokio` |

**为什么不能本地解决**: redis 0.27 锁定 socket2 0.5。tokio 生态使用 socket2 0.6。

**上游依赖**: `redis` 升级到 0.29（使用 `socket2 0.6`）

### 8. nom: 2 版本 (v7.1.3 / v8.0.0)

| 版本 | 引入路径 |
|------|----------|
| v7.1.3 | `config 0.14.1` |
| v8.0.0 | `av1-grain` (image processing) |

**为什么不能本地解决**: nom 7 和 8 是 breaking change。config 使用旧版，av1-grain 使用新版。

**上游依赖**: `config` 升级到 0.15+（使用 `nom 8`）

### 9. core-foundation: 2 版本 (v0.9.4 / v0.10.1) — macOS only

| 版本 | 引入路径 |
|------|----------|
| v0.9.4 | `security-framework` → macOS keychain |
| v0.10.1 | `core-foundation` 直接依赖 |

**为什么不能本地解决**: 仅 macOS 平台，两个版本存储开销极小。

**上游依赖**: `security-framework` 升级

---

## 整体影响评估

| 指标 | 值 |
|------|-----|
| 不同版本依赖组数 | 9 组（已排除平台特定） |
| 可本地解决 | 0 组（全部受 SemVer 约束） |
| 需上游升级 | 9 组 |
| 主要阻塞上游 | `config`(yaml-rust2), `dashmap`, `sqlx`, `vodozemac`, `redis`, `opentelemetry` |
| 编译体积影响 | 较小（Cargo 已做 LTO 和去重优化） |
| 运行时影响 | 无（链接时已去重） |

## 定期检查命令

```bash
# 每周检查可升级依赖
cargo update --dry-run

# 每月检查重复依赖变化
cargo tree -d --workspace | grep -E '^[a-zA-Z]' | sort -u

# 每季度检查过时依赖
cargo install cargo-outdated && cargo outdated -R
```

## 变更记录

| 日期 | 变更 |
|------|------|
| 2026-06-13 | 初始创建。消除 `base64` 重复（禁用 `config/ron`）。文档化剩余 9 组深层重复依赖。 |
