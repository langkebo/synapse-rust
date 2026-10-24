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

**统计口径（逐字可复现，2026-10-24 定稿）**：

1. `cargo tree -d --workspace --locked` 的输出里，**列 0 的行**（形如 `name vX.Y.Z`）是每个重复根；
2. 相邻的列 0 行**按 crate 名连续聚合**：同一名字的连续若干行算**一个** crate 名（`base64 v0.22.1` 出现两次 =
   同版本多来源，不是两个组）；
3. 该名字下**不同版本数 ≥ 2** ⇒ `版本分裂组`；**= 1** ⇒ `同版本多来源`；
4. 直接依赖者 = 该根下面缩进一层（`├──` / `└──`）的 crate；树里没展开的（已在别处 `(*)` 标记）用
   `cargo tree -i <crate>@<version> --depth 1 --workspace --locked` 补。

2026-10-24 实测：**101 个列 0 行 → 47 个 crate 名 → 32 个分裂组 + 15 个同版本多来源**。

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

> **§2 的逐族拆分已按 32 组重算（2026-10-24）**：族 A 16 / 族 B 4 / 族 C 3 / 族 D 2 / 族 E–K 各 1，
> 合计 32 组、11 族。与 10-04 的 10 族 / 38 组不可直接比较：消失的 8 组（`aes` 一族）与
> `socket2` 见 §1，新出现的 `deadpool` / `num-bigint` 见 §3.4。逐组引入者已按当前树重跑。

---

## 1. 已消除的重复依赖

| 依赖 | 旧分裂 | 解决方式 | 日期 | 现状 |
|------|--------|----------|------|------|
| `base64` | v0.21.7（via `ron` → `config`） | 禁用 `config` 的 `ron` feature，连带消除 `toml_datetime` / `toml_edit` / `winnow` | 2026-06-13 | ⚠️ 已**回归**：新出现 v0.22.1 / v0.23.1 分裂（详见 §3.6） |
| `socket2` | 0.5.10（via `redis 0.29.5`）/ 0.6.5 | `redis` 升到 1.x 后两侧都落在 `socket2 0.6` | 2026-10-24 复核确认 | ✅ **已消除**（`cargo tree -d` 里不再出现该 crate） |
| `aead` / `aes` / `chacha20` / `chacha20poly1305` / `cipher` / `inout` / `poly1305` / `universal-hash` | RustCrypto 旧一代（`aes 0.8` / `chacha20poly1305 0.10` / `cipher 0.4` …） | 本项目直连侧已升到新一代（`aes 0.9` / `chacha20poly1305 0.11` / `cipher 0.5`），不再双向并存 | 2026-10-24 复核确认 | ✅ **8 组已消除**（族 A 因此从 24 组降到 16 组，见 §3.1） |
| `typenum` | 同版本多来源（旧 §5 记为 16 个之一） | 依赖图收敛为单一来源 | 2026-10-24 复核确认 | ✅ **已消除**（§5 因此从 16 个降到 15 个） |

---

## 2. 根因族总览

**32 组版本分裂可归入 11 个根因族**（2026-10-24 按当前树重算，逐组见 §3）。其中 **A 族（RustCrypto 新老两代）仍是主体**：`vodozemac 0.11` 走新一代码，而 `sqlx 0.8` / `rsa 0.9` / `x509-cert 0.2` 仍停在旧一代码。

> ⚠️ 与 2026-10-04 的 10 族 / 38 组相比，本表是**重算结果**，不是平移：`aead` / `aes` / `chacha20` / `chacha20poly1305` / `cipher` / `inout` / `poly1305` / `universal-hash` 8 组**已消失**（本项目直连的 RustCrypto 依赖已升到新一代，见 §1），`socket2` 也已收敛；新增 `num-bigint`（`redis` 1.x 侧）与 `deadpool` / `deadpool-runtime`（`wiremock` 0.12 ↔ `deadpool-redis` 0.13）两个族。

| 族 | 主题 | 组数 | 优先级 |
|----|------|------|--------|
| A | RustCrypto 新老两代（`digest` 0.10↔0.11 / `sha2` 0.10↔0.11 / `hkdf` 0.12↔0.13 / `ed25519-dalek` 2↔3 …） | 16 | P1 |
| B | `rand` 三代并存（0.8 / 0.9 / 0.10）+ `getrandom` | 4 | P0 |
| C | `hashbrown` / `hashlink` / `foldhash` | 3 | P2 |
| D | `deadpool` / `deadpool-runtime`（`wiremock` ↔ `deadpool-redis`） | 2 | P3 |
| E | `num-bigint`（`sqlx-postgres`/`bigdecimal`/`simple_asn1` ↔ `redis` 1.x） | 1 | P2 |
| F | `nom`（`config` 锁 7，`lettre`/`av1-grain` 用 8） | 1 | P2 |
| G | `base64`（`lettre`/`vodozemac` 用 0.23） | 1 | P2 |
| H | `syn` 2↔3（proc-macro 生态换代） | 1 | P2 |
| I | `itertools`（`criterion` dev-only） | 1 | P3 |
| J | `core-foundation`（macOS only） | 1 | P3 |
| K | `libwebp-sys2`（图像 WebP 链） | 1 | P3 |

**族数为什么从 10 变 11**：老的 D 族（`socket2`）整族消失，新的 D/E 两族（`deadpool`、`num-bigint`）各自独立成族 —— 不是把老族拆开，而是分裂集合本身变了（`git log --oneline -- Cargo.lock` 可见 10-24 的 46 处版本变更）。

---

## 3. 版本分裂明细

> 「引入者」为 `cargo tree -i <crate>@<ver> --depth 1` 的直接依赖者；`●` 标记**本项目直连**的依赖（本地可主动升级的一侧）。

### 3.1 族 A — RustCrypto 新老两代（16 组）

**根因**：`vodozemac 0.11.0` 走新一代码（`hpke 0.14` → `digest 0.11` / `cipher 0.5` / `aead 0.6` / `sha2 0.11` / `curve25519-dalek 5` / `ed25519-dalek 3`）；而 `sqlx 0.8` / `rsa 0.9` / `x509-cert 0.2` 仍停在旧一代码（`digest 0.10` / `cipher 0.4` / `aead 0.5` / `sha2 0.10` / `curve25519-dalek 4` / `ed25519-dalek 2`）。

> **2026-10-24 重算**：本族从 24 组降到 **16 组**。`aead` / `aes` / `chacha20` / `chacha20poly1305` / `cipher` / `inout` / `poly1305` / `universal-hash` 8 组**已经收敛**（本项目直连的那一侧已升到新一代），不再是分裂项 —— 见 §1。下表逐行用 `cargo tree -d --workspace --locked` 的树形输出取直接依赖者（`●` = 本项目直连）。

| crate | 版本 | 直接依赖者（`cargo tree -i <crate>@<ver> --depth 1`） |
|-------|------|-----------|
| `block-buffer` | 0.10.4 | `digest 0.10.7` |
| | 0.12.1 | `cipher`、`digest 0.11.3` |
| `const-oid` | 0.9.6 | `der 0.7.10`、`digest 0.10.7`、`rsa 0.9.10`、`x509-cert 0.2.5` |
| | 0.10.2 | `der 0.8.1`、`digest 0.11.3` |
| `cpufeatures` | 0.2.17 | `sha2 0.10.9` |
| | 0.3.0 | `aes`、`polyval`、`sha1`、`sha2 0.11.0` |
| `crypto-common` | 0.1.7 | `digest 0.10.7` |
| | 0.2.2 | `aead`、`cipher`、`digest 0.11.3`、`elliptic-curve 0.14.1`、`primefield`、`universal-hash` |
| `curve25519-dalek` | 4.1.3 | `ed25519-dalek 2.2.0`（● 本项目直连 `ed25519-dalek 2.2.0`） |
| | 5.0.0 | `ed25519-dalek 3.0.0`、`vodozemac 0.11.0`、`x25519-dalek 3.0.0` |
| `der` | 0.7.10 | `pkcs1 0.7.5`、`pkcs8 0.10.2`、`spki 0.7.3`、`x509-cert 0.2.5` |
| | 0.8.1 | `ecdsa 0.17.0`、`pkcs8 0.11.0`、`sec1 0.8.1`、`spki 0.8.0` |
| `digest` | 0.10.7 | `curve25519-dalek 4.1.3`、`hmac 0.12.1`、`md-5 0.10.6`、`rsa 0.9.10`、`sha2 0.10.9`、`signature 2.2.0` |
| | 0.11.3 | `blake2 0.11.0`、`curve25519-dalek 5.0.0`、`ecdsa 0.17.0`、`elliptic-curve 0.14.1`、`hmac 0.13.0`、`sha1 0.11.0`、`sha2 0.11.0`、`signature 3.0.0` |
| `ed25519` | 2.2.3 | `ed25519-dalek 2.2.0` |
| | 3.0.0 | `ed25519-dalek 3.0.0` |
| `ed25519-dalek` | 2.2.0 | ● `synapse-common` / `synapse-e2ee` / `synapse-federation` / `synapse-rust` / `synapse-web` |
| | 3.0.0 | `vodozemac 0.11.0` |
| `hkdf` | 0.12.4 | `sqlx-postgres 0.8.6` |
| | 0.13.0 | `hpke 0.14.1`、`vodozemac 0.11.0`、● `synapse-common`、● `synapse-e2ee` |
| `hmac` | 0.12.1 | `hkdf 0.12.4`、`sqlx-postgres 0.8.6` |
| | 0.13.0 | `hkdf 0.13.0`、`rfc6979 0.6.0`、`vodozemac 0.11.0`、● `synapse-common` / `synapse-services` / `synapse-web` / `synapse-rust` |
| `pem-rfc7468` | 0.7.0 | `der 0.7.10` |
| | 1.0.0 | `der 0.8.1`、`elliptic-curve 0.14.1` |
| `pkcs8` | 0.10.2 | `pkcs1 0.7.5`、`rsa 0.9.10` |
| | 0.11.0 | `elliptic-curve 0.14.1` |
| `sha2` | 0.10.9 | `ed25519-dalek 2.2.0`、`rsa 0.9.10`、`sqlx-core`/`sqlx-postgres`/`sqlx-macros-core 0.8.6` |
| | 0.11.0 | `ed25519-dalek 3.0.0`、`hpke 0.14.1`、`p256 0.14.0`、`vodozemac 0.11.0`、● 多数 workspace crate（含 `synapse-test-utils`） |
| `signature` | 2.2.0 | `ed25519 2.2.3`、`rsa 0.9.10` |
| | 3.0.0 | `ecdsa 0.17.0`、`ed25519 3.0.0`、`ed25519-dalek 3.0.0` |
| `spki` | 0.7.3 | `pkcs1 0.7.5`、`pkcs8 0.10.2`、`rsa 0.9.10`、`x509-cert 0.2.5` |
| | 0.8.0 | `ecdsa 0.17.0`、`pkcs8 0.11.0` |

**消除路径（已变，2026-10-24）**：旧版写的"把本项目直连的旧一代同步升级"**已经做了一大半** —— `sha2` / `hkdf` / `hmac` 的直连侧现在都在**新**一代（0.11 / 0.13 / 0.13），旧一代只剩 `sqlx 0.8`（`sqlx-postgres` 的 `hkdf 0.12` / `hmac 0.12` / `sha2 0.10`）与 `rsa 0.9` 在拖。**唯一还能本地动的一条**是 `ed25519-dalek 2.2.0 → 3.0.0`（它由 5 个 workspace crate 直连，是 `curve25519-dalek 4` / `ed25519 2` / `signature 2` 这条链的根）。其余 15 组要等 `sqlx 0.9`（MSRV 1.94）与 `rsa 0.10` 发布。

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

### 3.4 族 D / E — `deadpool`（`wiremock` ↔ `deadpool-redis`）与 `num-bigint`（16 → 新增）

> 这一节在 2026-10-24 重算时是**新的**：旧版的族 D 是 `socket2`，它已经收敛（见 §1）。当前树里 2+1 个组的根因与 `socket2` 无关，故按实际引入者重新归类。

| crate | 版本 | 直接依赖者 |
|-------|------|-----------|
| `deadpool` | 0.12.3 | `wiremock`（dev-dependency，mock HTTP 服务器） |
| | 0.13.1 | `deadpool-redis`（本项目 `redis` 连接池） |
| `deadpool-runtime` | 0.1.4 | `deadpool 0.12.3` |
| | 0.3.1 | `deadpool 0.13.1` |
| `num-bigint` | 0.4.8 | `bigdecimal`、`num-rational`、`simple_asn1`、`sqlx-postgres 0.8.6` |
| | 0.5.1 | `redis 1.x` |

**消除路径**：`deadpool` 一组**只影响 dev**（`wiremock` 生产不消费），优先级 P3；`num-bigint` 需 `redis` 与 `sqlx` 两侧对齐 `num-bigint` 主版本，本地不可动。**检查频率**：每季度

### 3.5 族 F / G / H — `nom` / `base64` / `syn`

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

### 3.6 族 I / J / K — `itertools` / `core-foundation` / `libwebp-sys2`

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
| 版本分裂组数 | 32 组（2026-10-24 复核；§2 的逐族拆分已按同一份数据重算：16 / 4 / 3 / 2 / 1×7） |
| 可本地 `[patch]` 解决 | 0 组（全部跨主版本） |
| 需上游升级 | 31 组（唯一可本地收窄的是 `ed25519-dalek 2 → 3` 那条链，见 §3.1） |
| 最高杠杆本地动作 | 升级本项目直连的旧一代 RustCrypto（族 A）+ 统一 `rand` 到单一主版本（族 B） |
| 主要阻塞上游 | `sqlx 0.8`、`rsa 0.9`、`argon2 0.5`、`vodozemac 0.11`、`redis 0.29`、`config 0.14`、`dashmap 6`、`opentelemetry` |
| 编译体积影响 | 较小（Cargo 已做去重/LTO，重复主要体现为编译时间与 target 目录体积） |
| 运行时影响 | 无（链接期已去重，各版本独立符号） |

## 5. 同版本多来源出现（非 SemVer 分裂，不跟踪）

以下 15 个 crate 被 `cargo tree -d` 以**同一版本**多次列出（同版本、不同依赖路径来源），不属于版本分裂，不构成本跟踪表的治理对象：

`bitflags`、`byteorder`、`chrono`、`fastrand`、`futures-channel`、`futures-sink`、`futures-util`、`log`、`num-traits`、`slab`、`smallvec`、`sqlx-postgres`、`subtle`、`tokio`、`uuid`

> 2026-10-24 复核：旧表的 16 个里 `typenum` 已收敛为单一来源（见 §1），其余 15 个不变。

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
| 2026-10-24（W8 / NEW-03） | **§2 逐族表按 32 组重算，并把「哪 6 组消失了」回答掉**：`aead` / `aes` / `chacha20` / `chacha20poly1305` / `cipher` / `inout` / `poly1305` / `universal-hash` 8 组已收敛（本项目直连侧升到新一代），`socket2` / `typenum` 也收敛 ⇒ §1 逐条登记；新增 `deadpool` / `deadpool-runtime` / `num-bigint` 三组归入新的族 D/E（§3.4）。族数 10 → 11、族 A 24 → **16**、`需上游升级` 32 → **31**（`ed25519-dalek 2→3` 是唯一还能本地收窄的链）。§3.1 的 14 行逐组引入者全部按 `cargo tree -i <crate>@<ver> --depth 1 --workspace --locked` 重跑，订正了旧表里「本项目直连旧一代 RustCrypto」的整体判断（`sha2` / `hkdf` / `hmac` 直连侧现在都在**新**一代）。**统计口径**：树形输出的列 0 行按 crate 名连续聚合（`101` 个列 0 行 → `47` 名 → `32` 分裂组 + `15` 同版本多来源），脚本与命令写在 §0。 |
