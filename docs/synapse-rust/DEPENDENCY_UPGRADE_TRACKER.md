# DEPENDENCY_UPGRADE_TRACKER.md

> 深层重复依赖跟踪表，记录无法通过本地 `[patch]` 解决的 SemVer 不兼容重复依赖。
> 最后更新: 2026-10-21
> 基线: `cargo tree -d --workspace`（在 `cargo update` 后、无本地 `[patch]` 覆盖）
> 权威口径: 本文件是「SemVer 不兼容深层重复依赖」的**唯一权威来源**；`project_rules.md §17.5` 仅作摘要与指引。

---

## 0. 复现命令

```bash
# 列出全部重复依赖组（版本分裂 + 同版本多来源）
cargo tree -d --workspace

# 查看某个重复 crate 的反向依赖（谁引入了它），--depth 1 只看直接依赖者
cargo tree -i <crate>@<version> --depth 1 --workspace
```

**当前基线统计（2026-10-24 复核）：**

> 复核方式：`cargo tree -d --workspace --locked` 按 crate 名聚合。产出 **47** 个 crate 名，其中
> **32 个真正多版本**、**15 个同版本多来源**。
> ⚠️ **`--workspace` 不可省**：不加时实测是 80 个 crate 名 / 31 组 —— 那是另一个口径（少了成员 crate 的
> dev 依赖），与本节数字不可比。2026-10-24 第一遍就是不加该 flag，得出了"38 → 31"的错误结论。
> ⚠️ 也不要用 `Cargo.lock` 直接数「同名包的 major.minor 不同」来复算本表：该口径会把「同版本多来源」
> 和 tree 的目标差异化结果混进来（实测会得到 51，是错的）。

| 指标 | 值 |
|------|-----|
| 版本分裂组（同名 ≥2 个不同版本） | **32 组**（2026-10-21 记 38；`Cargo.lock` 在 10-24 有 46 处版本变更） |
| 同版本多来源出现（非 SemVer 分裂） | 15 个 crate（见 §5；2026-10-21 记 16） |
| 可本地 `[patch]` 解决 | 0 组（全部跨主版本，`[patch]` 只能锁定同主版本） |
| 最大根因 | `vodozemac 0.11`（新一代码）与本项目直连的旧一代 RustCrypto 并存 |

> **§2 的"10 个根因族"表尚未按 32 组重算**（各组计数仍是 38 组时的快照）。组数下降 6 组、
> 来源数下降 1 个，方向是好的，但"哪 6 组消失了、是否被上游升级解决"需要逐族重跑才能回答 ——
> 在那之前，§2 的**逐族组数只当历史**，不要拿它做进度判断。审计 NEW-03 记的就是这件事。

---

## 1. 已消除的重复依赖

| 依赖 | 旧分裂 | 解决方式 | 日期 | 现状 |
|------|--------|----------|------|------|
| `base64` | v0.21.7（via `ron` → `config`） | 禁用 `config` 的 `ron` feature，连带消除 `toml_datetime` / `toml_edit` / `winnow` | 2026-06-13 | ⚠️ 已**回归**：新出现 v0.22.1 / v0.23.1 分裂（详见 §3.6） |

---

## 2. 根因族总览

38 组版本分裂可归入 10 个根因族。其中 **A 族（RustCrypto 新老两代）是绝对主体**，由 `vodozemac 0.11`（新一代码）与本项目直连的旧一代码共同造成。

| 族 | 主题 | 组数 | 优先级 |
|----|------|------|--------|
| A | RustCrypto 新老两代（`digest` 0.10↔0.11 / `cipher` 0.4↔0.5 / `aead` 0.5↔0.6 …） | 24 | P1 |
| B | `rand` 三代并存（0.8 / 0.9 / 0.10）+ `getrandom` | 4 | P0 |
| C | `hashbrown` / `hashlink` / `foldhash` | 3 | P2 |
| D | `socket2`（`redis` 锁旧版） | 1 | P1 |
| E | `nom`（`config` 锁 7，`lettre`/`av1-grain` 用 8） | 1 | P2 |
| F | `base64`（`lettre`/`vodozemac` 用 0.23） | 1 | P2 |
| G | `syn` 2↔3（proc-macro 生态换代） | 1 | P2 |
| H | `itertools`（`criterion` dev-only） | 1 | P3 |
| I | `core-foundation`（macOS only） | 1 | P3 |
| J | `libwebp-sys2`（图像 WebP 链） | 1 | P3 |

---

## 3. 版本分裂明细

> 「引入者」为 `cargo tree -i <crate>@<ver> --depth 1` 的直接依赖者；`●` 标记**本项目直连**的依赖（本地可主动升级的一侧）。

### 3.1 族 A — RustCrypto 新老两代

**根因**：`vodozemac 0.11.0` 走新一代码（`hpke 0.14` → `digest 0.11` / `cipher 0.5` / `aead 0.6` / `sha2 0.11` / `curve25519-dalek 5` / `ed25519-dalek 3`）；而本项目与 `sqlx 0.8` / `rsa 0.9` / `argon2 0.5` 仍停在旧一代码（`digest 0.10` / `cipher 0.4` / `aead 0.5` / `sha2 0.10` / `curve25519-dalek 4` / `ed25519-dalek 2`）。

| crate | 版本 | 主要引入者 |
|-------|------|-----------|
| `aead` | 0.5.2 | `aes-gcm 0.10.3`、`chacha20poly1305 0.10.1` |
| | 0.6.1 | `chacha20poly1305 0.11.0`、`hpke 0.14.1` |
| `aes` | 0.8.4 | `aes-gcm 0.10.3`、● `synapse-e2ee` |
| | 0.9.3 | `vodozemac 0.11.0` |
| `block-buffer` | 0.10.4 | `digest 0.10.7` |
| | 0.12.1 | `cipher 0.5.2`、`digest 0.11.3` |
| `chacha20` | 0.9.1 | `chacha20poly1305 0.10.1` |
| | 0.10.2 | `chacha20poly1305 0.11.0`、`rand 0.10.2` |
| `chacha20poly1305` | 0.10.1 | ● `synapse-e2ee` |
| | 0.11.0 | `hpke 0.14.1`、`vodozemac 0.11.0` |
| `cipher` | 0.4.4 | `aes 0.8.4`、`aes-gcm 0.10.3`、`chacha20 0.9.1`、`chacha20poly1305 0.10.1`、`ctr` |
| | 0.5.2 | `aes 0.9.3`、`cbc 0.2.1`、`chacha20 0.10.2`、`chacha20poly1305 0.11.0`、`vodozemac 0.11.0` |
| `const-oid` | 0.9.6 | `der 0.7.10`、`digest 0.10.7`、`rsa 0.9.10`、`x509-cert 0.2.5` |
| | 0.10.2 | `der 0.8.1`、`digest 0.11.3` |
| `cpufeatures` | 0.2.17 | `aes 0.8.4`、`polyval`、`sha1`、`sha2 0.10.9` |
| | 0.3.0 | `aes 0.9.3`、`sha2 0.11.0` |
| `crypto-common` | 0.1.7 | `aead 0.5.2`、`cipher 0.4.4`、`digest 0.10.7`、`universal-hash 0.5.1` |
| | 0.2.2 | `aead 0.6.1`、`cipher 0.5.2`、`digest 0.11.3`、`elliptic-curve 0.14.1`、`primefield`、`universal-hash 0.6.1` |
| `curve25519-dalek` | 4.1.3 | `ed25519-dalek 2.2.0`（● 本项目直连 `ed25519-dalek 2.2.0`） |
| | 5.0.0 | `ed25519-dalek 3.0.0`、`vodozemac 0.11.0`、`x25519-dalek 3.0.0` |
| `der` | 0.7.10 | `pkcs1 0.7.5`、`pkcs8 0.10.2`、`spki 0.7.3`、`x509-cert 0.2.5` |
| | 0.8.1 | `ecdsa 0.17.0`、`pkcs8 0.11.0`、`sec1 0.8.1`、`spki 0.8.0` |
| `digest` | 0.10.7 | `blake2 0.10.6`、`curve25519-dalek 4.1.3`、`hmac 0.12.1`、`rsa 0.9.10`、`sha1` |
| | 0.11.3 | `curve25519-dalek 5.0.0`、`ecdsa 0.17.0`、`elliptic-curve 0.14.1`、`hmac 0.13.0`、`sha2 0.11.0` |
| `ed25519` | 2.2.3 | `ed25519-dalek 2.2.0` |
| | 3.0.0 | `ed25519-dalek 3.0.0` |
| `ed25519-dalek` | 2.2.0 | ● 本项目直连（`synapse-common` / `synapse-e2ee` / `synapse-federation` / `synapse-rust` / `synapse-web`） |
| | 3.0.0 | `vodozemac 0.11.0` |
| `hkdf` | 0.12.4 | `sqlx-postgres 0.8.6`、● `synapse-common`、● `synapse-e2ee` |
| | 0.13.0 | `hpke 0.14.1`、`vodozemac 0.11.0` |
| `hmac` | 0.12.1 | `hkdf 0.12.4`、`sqlx-postgres 0.8.6`、● 多个 workspace crate |
| | 0.13.0 | `hkdf 0.13.0`、`rfc6979 0.6.0`、`vodozemac 0.11.0` |
| `inout` | 0.1.4 | `cipher 0.4.4` |
| | 0.2.2 | `aead 0.6.1`、`cipher 0.5.2` |
| `pem-rfc7468` | 0.7.0 | `der 0.7.10` |
| | 1.0.0 | `der 0.8.1`、`elliptic-curve 0.14.1` |
| `pkcs8` | 0.10.2 | `pkcs1 0.7.5`、`rsa 0.9.10` |
| | 0.11.0 | `elliptic-curve 0.14.1` |
| `poly1305` | 0.8.0 | `chacha20poly1305 0.10.1` |
| | 0.9.1 | `chacha20poly1305 0.11.0` |
| `sha2` | 0.10.9 | `ed25519-dalek 2.2.0`、`rsa 0.9.10`、`sqlx-core`/`sqlx-postgres 0.8.6`、● 多个 workspace crate |
| | 0.11.0 | `ed25519-dalek 3.0.0`、`hpke 0.14.1`、`p256 0.14.0`、`vodozemac 0.11.0` |
| `signature` | 2.2.0 | `ed25519 2.2.3`、`rsa 0.9.10` |
| | 3.0.0 | `ecdsa 0.17.0`、`ed25519 3.0.0`、`ed25519-dalek 3.0.0` |
| `spki` | 0.7.3 | `pkcs1 0.7.5`、`pkcs8 0.10.2`、`rsa 0.9.10`、`x509-cert 0.2.5` |
| | 0.8.0 | `ecdsa 0.17.0`、`pkcs8 0.11.0` |
| `universal-hash` | 0.5.1 | `poly1305 0.8.0`、`polyval 0.6.2` |
| | 0.6.1 | `poly1305 0.9.1` |

**消除路径**：把本项目直连的旧一代 RustCrypto 依赖同步升级到新一代（`aes` 0.9 / `chacha20poly1305` 0.11 / `sha2` 0.11 / `hkdf` 0.13 / `hmac` 0.13 / `ed25519-dalek` 3），再等待 `sqlx 0.8`、`rsa 0.9`、`argon2 0.5` 跟进新一代。**这是当前最高杠杆的单一动作**——因为旧一代的多个版本（`aes 0.8`/`chacha20poly1305 0.10`/`sha2 0.10`/`hkdf 0.12`/`hmac 0.12`/`ed25519-dalek 2`）正是由本项目的直接依赖引入的。

**检查频率**：每月

### 3.2 族 B — `rand` 三代并存

| crate | 版本 | 主要引入者 |
|-------|------|-----------|
| `rand` | 0.8.7 | `sqlx-postgres 0.8.6`、`num-bigint-dig 0.8.6`（via `rsa 0.9.10`） |
| | 0.9.5 | `opentelemetry_sdk 0.31.0`、● 多个 workspace crate |
| | 0.10.2 | `quickcheck 1.1.0`（dev）、`vodozemac 0.11.0` |
| `rand_core` | 0.6.4 | `crypto-common 0.1.7`、`password-hash 0.5.0`、`rand 0.8.7`、`rand_chacha 0.3.1`、`rsa 0.9.10`、`signature 2.2.0` |
| | 0.9.5 | `rand 0.9.5`、`rand_chacha 0.9.0` |
| | 0.10.1 | `crypto-bigint`、`crypto-common 0.2.2`、`curve25519-dalek 5.0.0`、`ed25519-dalek 3.0.0`、`elliptic-curve 0.14.1`、`ff`、`getrandom`、`group`、`hpke 0.14.1`、`primefield`、`rand 0.10.2`、`signature 3.0.0`、`x25519-dalek 3.0.0` |
| `rand_chacha` | 0.3.1 | `rand 0.8.7` |
| | 0.9.0 | `rand 0.9.5` |
| `getrandom` | 0.2.17 | `rand_core 0.6.4`、`ring` |
| | 0.3.4 | `rand_core 0.9.5` |
| | 0.4.3 | `crypto-bigint`、`crypto-common 0.2.2`、`hpke 0.14.1`、`rand 0.10.2`、`tempfile`、`uuid`、`vodozemac 0.11.0` |

**消除路径**：`rand` 0.8（旧一代 RustCrypto/sqlx）与 0.10（新一代码）不受本项目控制；本项目可先把直连的 `rand` 统一到与 `vodozemac 0.11` 相同的 **0.10**（或等 `opentelemetry` 支持）。`getrandom` 随 `rand` 链自动收敛。

**0.8 分支的父节点全在上游手里（2026-10-21 实测）**：

- `fake 2.10.0` 曾是本族第三个父节点，但**全仓没有任何代码引用它**（`fake::`、`use fake`、`Dummy` derive 全部零命中，只有本项目自有的 `FakeUserStore` 同名前缀）。已作为未使用 dev-dependency 删除，连带的 `deunicode` 一并消失（包总数 623 → 621）；`rand 0.8.7` 的父节点因此只剩 `num-bigint-dig`（via `rsa 0.9.10`）与 `sqlx-postgres 0.8.6`。
- `sqlx 0.9.0` 已经切到 `rand 0.10.1` 与 `rsa 0.10.0-rc.18`（上游注明 `FIXME: when rsa 0.10 is released`），但 **MSRV = 1.94.0**，高于本项目 `rust-toolchain.toml` 的 1.93.0 —— 升 `sqlx` 前必须先升工具链，且 0.9 是大版本（feature 名重排、`sqlite` 拆成 `sqlite-bundled`/`sqlite-unbundled`）。
- `rsa` 稳定版仍是 0.9.10（0.10 只有 `0.10.0-rc.19` 预发布）。本项目 `rsa` 是 `synapse-services` 的直接依赖，升到 rc 需自行承担 API 稳定性风险。

`argon2` **不是** `rand 0.8` 的直接父节点（它经由 `password-hash` 落在 `rand_core 0.6`），因此升级 `argon2` 不会缩短 0.8 这一代。

**检查频率**：每月

### 3.3 族 C — `hashbrown` / `hashlink` / `foldhash`

| crate | 版本 | 主要引入者 | 根本原因 |
|-------|------|-----------|----------|
| `hashbrown` | 0.14.5 | `dashmap 6.2.1`、`hashlink 0.8.4` | `dashmap 6.x` 锁 `hashbrown 0.14` |
| | 0.15.5 | `hashlink 0.10.0`、`sqlx-core 0.8.6` | `sqlx` 用 `hashlink 0.10` |
| | 0.17.1 | `indexmap 2.14.0`、`lru 0.18.4` | `indexmap 2.x` 升级到 `hashbrown 0.17` |
| `hashlink` | 0.8.4 | `yaml-rust2 0.8.1`（via `config 0.14.1`） | `config` 依赖旧版 yaml 解析器 |
| | 0.10.0 | `sqlx-core 0.8.6` | `sqlx` 用新版 |
| `foldhash` | 0.1.5 / 0.2.0 | 由 `hashbrown` 0.14/0.15 与 0.17 分别引入 | 随 `hashbrown` 分裂 |

**消除路径**：需上游统一（`dashmap`↔`config`↔`sqlx`↔`indexmap`），短期不可行。

**检查频率**：每月

### 3.4 族 D — `socket2`

| 版本 | 主要引入者 |
|------|-----------|
| 0.5.10 | `redis 0.29.5` |
| 0.6.5 | `dns-lookup`、`hyper-util`、`lettre`、`tokio` |

**消除路径**：升级 `redis` 到 1.x（改用 `socket2 0.6`），属独立 API 迁移任务。

**检查频率**：每季度

### 3.5 族 E / F / G — `nom` / `base64` / `syn`

| crate | 版本 | 主要引入者 |
|-------|------|-----------|
| `nom` | 7.1.3 | `config 0.14.1` |
| | 8.0.0 | `av1-grain`、`lettre` |
| `base64` | 0.22.1 | `sqlx-core`/`sqlx-postgres 0.8.6`、`reqwest`、`jsonwebtoken`、`tonic`、`pem`、`hyper-util`、`wiremock`、● 多数 workspace crate |
| | 0.23.1 | `lettre`、`email-encoding`、`vodozemac 0.11.0` |
| `syn` | 2.0.119 | `axum-macros`、`darling`、`derive_arbitrary`、`mockall_derive`、`prost-derive`、`quickcheck_macros`、`sqlx-macros`、`thiserror`（旧）等大量 proc-macro |
| | 3.0.3 | `async-trait`、`displaydoc`、`educe`、`enum-ordinalize-derive`、`proc-macro-error3`、`serde_derive`、`thiserror-impl`、`tokio-macros` |

**`nom` 消除路径**：`config` 升级到 0.15+（改用 `nom 8`），或 `lettre` 降级。**检查频率**：每季度
**`base64` 消除路径**：待 `lettre`/`vodozemac` 与 `sqlx`/`reqwest` 两侧对齐同一主版本；本项目直连的 `base64 0.22` 可在其余依赖就绪后升到 0.23。**检查频率**：每季度
**`syn` 消除路径**：纯 proc-macro 生态换代，随各宏 crate 自然收敛，无本地动作。**检查频率**：每季度

### 3.6 族 H / I / J — `itertools` / `core-foundation` / `libwebp-sys2`

| crate | 版本 | 主要引入者 | 备注 |
|-------|------|-----------|------|
| `itertools` | 0.10.5 | `criterion 0.5.1`（dev）、`criterion-plot` | 仅 dev-dependency，不进生产二进制 |
| | 0.14.0 | `prost-derive`、`rav1e` | |
| `core-foundation` | 0.9.4 | `system-configuration` | **仅 macOS** |
| | 0.10.1 | `security-framework` | |
| `libwebp-sys2` | 0.1.11 / 0.2.0 | 图像 WebP 链（`webp` / `webp-animation 0.10.0` / `image`） | 存储开销极小 |

**消除路径**：`itertools` 待 `criterion` 0.6+（dev-only）；`core-foundation` 待 `security-framework` 与 `system-configuration` 统一；`libwebp-sys2` 待 `webp-animation` 升级。**检查频率**：每季度

---

## 4. 整体影响评估

| 指标 | 值 |
|------|-----|
| 版本分裂组数 | 32 组（2026-10-24 复核；§2 的逐族拆分仍是 38 组时的快照） |
| 可本地 `[patch]` 解决 | 0 组（全部跨主版本） |
| 需上游升级 | 32 组 |
| 最高杠杆本地动作 | 升级本项目直连的旧一代 RustCrypto（族 A）+ 统一 `rand` 到单一主版本（族 B） |
| 主要阻塞上游 | `sqlx 0.8`、`rsa 0.9`、`argon2 0.5`、`vodozemac 0.11`、`redis 0.29`、`config 0.14`、`dashmap 6`、`opentelemetry` |
| 编译体积影响 | 较小（Cargo 已做去重/LTO，重复主要体现为编译时间与 target 目录体积） |
| 运行时影响 | 无（链接期已去重，各版本独立符号） |

## 5. 同版本多来源出现（非 SemVer 分裂，不跟踪）

以下 16 个 crate 被 `cargo tree -d` 以**同一版本**多次列出（同版本、不同依赖路径来源），不属于版本分裂，不构成本跟踪表的治理对象：

`bitflags`、`byteorder`、`chrono`、`fastrand`、`futures-channel`、`futures-sink`、`futures-util`、`log`、`num-traits`、`slab`、`smallvec`、`sqlx-postgres`、`subtle`、`tokio`、`typenum`、`uuid`

## 6. 定期检查命令

```bash
# 每周：检查可升级依赖
cargo update --dry-run

# 每月：检查重复依赖变化（权威口径见本文件 §0）
cargo tree -d --workspace

# 每季度：检查过时依赖 / 安全审计
cargo install cargo-outdated && cargo outdated -R
cargo audit
```

## 7. 变更记录

| 日期 | 变更 |
|------|------|
| 2026-06-13 | 初始创建。消除 `base64` 0.21.7 重复（禁用 `config/ron`）。记录 9 组深层重复。 |
| 2026-10-04 | **全量重写为实测口径**：以 `cargo tree -d --workspace` 重新基线，收敛为 **38 组版本分裂**（按 10 个根因族归类，附直接依赖者）；订正旧版严重过时的版本号（`vodozemac` 0.9→0.11、`redis` 0.27→0.29 等）；标记族 A/B 为最高杠杆本地动作；记录 `base64` 分裂回归；新增 §5 同版本多来源说明；确立本文件为唯一权威口径。 |
| 2026-10-14 | **安全升级 `rustls` 0.23.45 → 0.23.46**（`cargo update -p rustls --precise`），消除**当日发布**的 4 条漏洞：`RUSTSEC-2026-0346`（`Debug` 实现泄漏 TLS 会话秘密）、`RUSTSEC-2026-0347`（ECH 被拒后仍向外部服务器发送已缓冲应用数据）、`RUSTSEC-2026-0348`（Brotli 证书解压可在认证前分配至 1 GiB）、`RUSTSEC-2026-0349`（未认证 ServerHello 可把后续连接降级到经典密钥交换）。四条修复区间同为 `>= 0.23.46`。rustls 是**传递依赖**（`lettre`（SMTP）、`redis` TLS），清单里没有可钉的位置，防降级由 CI 的 supply-chain gate 每次重跑覆盖。 |
| 2026-10-14 | **消除 `RUSTSEC-2024-0449`**（`unsafe-libyaml` 未维护，经 `serde_yaml 0.9` 传入）：把 `serde_yaml 0.9` 迁到维护中的同源代码 fork **`serde_norway 0.9.42`**（底层 `unsafe-libyaml-norway 0.2.15`）。调用面 49 处 / 14 个 `.rs`（`from_str` 32、`to_string` 13、`Value` 2、`Error` 1），全仓无 `use serde_yaml` 别名 ⇒ 纯文本替换。`Cargo.lock` 恰 20 增 / 20 删（只换那两个 package 块）。**未在 `deny.toml` / `.cargo/audit.toml` 新增任何 ignore** —— 是消除而非遮蔽。结果：`cargo deny check advisories` = `advisories ok`、`cargo audit` EXIT=0（1314 条 advisory，零命中）⇒ **供应链门禁首次转绿**。 |
| 2026-10-14 | **安全升级 `ammonia` 4.1.4 → 4.2.3**，消除 `RUSTSEC-2021-0157`（CSS `url("")` 转义注入）与 `RUSTSEC-2021-0158`（SVG `animateMotion`/`animateTransform`/`animateColor` XSS）。`"4.1"` 是 caret 语义 ⇒ 连带升级 `cssparser` 0.37→0.38、`html5ever` 0.39→0.40.1、`markup5ever` 0.39→0.40、`phf`×4 0.13.1→0.14、`string_cache` 0.9→0.11、`string_cache_codegen`、`web_atoms`，**共 11 个包**；依赖图核查确认它们**只属 ammonia → html5ever → markup5ever → web_atoms 这一条链**，workspace 内无其他消费者 ⇒ 影响面收敛在净化器。清单下限显式抬到 `"4.2.3"`，防止 `-Z minimal-versions` 或一次 lock 重建把依赖降回脆弱版本。行为由新增 golden（`tests/unit/sanitizer_golden_tests.rs`，45 例，含两条 advisory 的 PoC 形状）锁定为**升级前后逐字节一致**。**遗留 1 条**：`cargo deny` 当时仍报 `RUSTSEC-2024-0449`（`unsafe-libyaml` 未维护，经 `serde_yaml 0.9` 传入；无已知漏洞，消除需迁出 serde_yaml）—— **2026-10-14 已消除**，见本表同日的 `RUSTSEC-2024-0449` 行。 |
| 2026-10-24 | **重算版本分裂组数：38 → 32 组**（crate 名 54 → 47、同版本多来源 16 → 15；口径 `cargo tree -d --workspace --locked` 按 crate 名聚合）。组数下降 6 的来源未逐族归因 —— §2 的逐族表仍是旧快照，见 §0 的说明。同时把「必须带 `--workspace`」写进复现命令：不加时实测 80 名 / 31 组，是不可比的口径（审计 NEW-03）。 |
