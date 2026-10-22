# 第二批工程改造：评估与执行顺序（2026-10-21）

> 本文件回答"第二批 5 项能不能做、先做什么、各会自动碰哪些东西"。
> 所有版本号都是 **2026-10-21 用 crates.io API 实测**，不是记忆；影响面是 `*.rs` 实测计数
> （排除 `target/`、`vendor/`、`.worktrees/`）。
> **现状（2026-10-22 更新）**：第 1 项（RustCrypto）已按 P-A…P-E 全部落地；第 2 项（redis）已完成；
> 第 3 项的 §5.4 已在静默窗口复测（结论见 §3）；第 4 项（A3 路由退役）**仍被外部占用挡住**；
> 第 5 项（L-7 全量）已在夜间启动。执行记录与提交号见 §7。

## 1. RustCrypto 六族

### 1.1 现状 → 目标

| 族 | crate | 现在 | 目标（max_stable） | 本批 |
|---|---|---|---|---|
| hash | `sha1` | 0.10 | **0.11.0** | ✅ |
| hash | `sha2` | 0.10 | **0.11.1** | ✅ |
| mac | `hmac` | 0.12 | **0.13.0** | ✅ |
| kdf | `hkdf` | 0.12 | **0.13.0** | ✅ |
| password-hash | `argon2` | 0.5 | **0.6.0** | ✅ |
| aead | `aes-gcm` | 0.10 | **0.11.1** | ✅ |
| aead | `chacha20poly1305` | 0.10 | **0.11.0** | ✅ |
| block | `aes` | 0.8 | **0.9.3** | ✅ |
| block | `ctr` | 0.9 | **0.10.1** | ✅ |
| signature | `rsa` | 0.9 | 0.10.0-**rc.19** | ❌ 不发版 |
| signature | `p256` | 0.14 | 0.14.0（已最新） | ❌ 不动 |
| vendored | `vodozemac`（`synapse-e2ee` 自带） | — | — | ❌ 不动 |

`rsa 0.10` 仍停在 RC（第 19 个 rc）：按"间接依赖留观"的同一口径，**本批不碰这一族**，
等 0.10.0 正式发版再单独一批（它同时换 `signature`/`pkcs1`/`pkcs8` 三代 trait，风险与其余五族不同量级）。

### 1.2 已知 breaking changes（逐条带出处）

| # | 变化 | 影响 | 出处 |
|---|---|---|---|
| 1 | `digest` 0.11 的 `finalize()` 返回 `hybrid_array::Array`，**不再实现 `LowerHex`** | `format!("{:x}", digest)` 编译失败，需逐字节 hex 或 `base16ct` | smithy-rs #4587 迁移记录 + 第三方迁移实录 |
| 2 | `digest::crypto_common` 移除 ⇒ `digest::common` | 直接 `use crypto_common::…` 的路径要改 | RustCrypto/hashes `sha2 v0.11.0-rc.5` commit message |
| 3 | `hmac` 0.13：`new_from_slice` 从 `Mac` trait **移到 `KeyInit`** | 每个用到的地方要 `use hmac::KeyInit`（机械，但漏一个就报 cannot find function） | smithy-rs #4587 明确列出 |
| 4 | `hkdf` 0.13：移除 `std` feature、MSRV 1.81、依赖 hmac 0.13 | 若有 `features = ["std"]` 要去掉 | hkdf CHANGELOG 0.13 |
| 5 | `aes-gcm` 0.11：`generate_key(OsRng)` / `generate_nonce` → `Key::generate()` / `Nonce::generate()` | **落在密钥/随机数生成路径**，27 处 | aes-gcm 0.11 迁移说明 |
| 6 | `sha2` 0.11：硬件 SHA 运行时自动探测（不再需要 `asm` feature） | 纯收益（aarch64/x86 吞吐显著提升，M 系列尤其） | 同上 smithy-rs 实测数据 |

`argon2 0.6` 的具体 breaking **未核实**（只确认了 0.6.0 已发版）。实施前必须先读
`password-hashes` 的 CHANGELOG，不要凭 0.5 的经验直接改 —— 这一条是本文件里唯一的"待查"。

### 1.3 本仓影响面（实测）

| 模式 | 处数 | 文件数 | 说明 |
|---|---|---|---|
| `.finalize()` | 33 | 22 | 含 `Mac::finalize`（返回 `CtOutput`，**不变**）⇒ 只有 digest 那部分要改 |
| `new_from_slice(` | 13 | 10 | hmac 0.13 需补 `KeyInit` import（§1.2 #3） |
| `generate_key` / `generate_nonce` | 27 | 6 | aes-gcm 0.11（§1.2 #5） |
| `GenericArray` | 6 | 2 | digest 0.11 直接受影响 |
| `Sha256::digest(` / `Sha1::digest(` | 3 | 3 | 返回类型变化 |
| `Argon2` | 42 | 12 | 集中在口令哈希路径 |
| `aes_gcm::` | 5 | 3 | |
| `Hkdf` | 2 | 2 | |
| `chacha20poly1305::` | 2 | 1 | |

### 1.4 分批（每批一提交，独立可回滚）

| 批 | 内容 | 为什么单独一批 |
|---|---|---|
| **P-A** | `sha1` + `sha2` + `hmac` | digest 0.11 是其余族的底层；hex 输出与 33 处 finalize 一起改，diff 才能自洽 |
| **P-B** | `hkdf` + `argon2` | kdf 面；argon2 的 CHANGELOG 要先读 |
| **P-C** | `aes-gcm` + `chacha20poly1305` | **碰加密路径**：必须有"升级前后密文可互通"的证据（同一 KAT 向量/同一密文解密） |
| **P-D** | `aes` + `ctr` | block 面，使用面窄 |
| **P-E** | 收尾：`rsa` 留 RC、`p256` 不动、`vodozemac` 不动，把理由写进 `Cargo.toml` 注释 | 免得下次又被当"漏网的旧依赖" |

每批验收：`cargo clippy --workspace --all-targets --features test-utils --all-features -- -D warnings`
+ `nextest --test unit --features test-utils` + 受影响的集成子集 + `cargo deny check`。

**P-C 的硬要求**：升级前先用现状代码加密若干固定输入并落盘（或用现有 KAT 向量），升级后解密必须一致。
若仓库里没有 KAT，这一步先补测试再升级 —— 否则 `generate_*` 的改动只能靠"能编译 + 能自解密"糊过去。

## 2. redis 0.29 → 1.x

### 2.1 依赖矩阵（实测）

| 项 | 现状 | 目标 | 证据 |
|---|---|---|---|
| `redis` | 0.29（lock 0.29.5） | **1.7.1** | crates.io max_stable |
| `deadpool-redis` | 0.20 | **0.23.1** | 其 `redis` 依赖为 `^1.6` ⇒ 必须一起动 |
| `socket2` 0.5.10 | 唯一父节点是 `redis 0.29.5` | 随 redis 升级消失 | `cargo tree -i socket2@0.5.10 --depth 1` |
| `tokio-rustls-comp` feature | 在用 | **1.x 仍保留** | redis 1.7.1 的 features 实测 |

### 2.2 redis 1.0 的 breaking（官方 `version1.md`）

| # | 变化 | 本仓是否受影响 |
|---|---|---|
| 1 | `FromRedisValue::from_redis_value` 改收 **owned `Value`** | **不受影响**：全仓 `impl FromRedisValue` = **0 处**（实测） |
| 2 | 反序列化错误换新类型；**错误文案变化**（若代码/测试 match 文案要改） | 实施时 grep 断言文案 |
| 3 | 移除 async-std 运行时支持 | 不受影响（tokio） |
| 4 | 新增：fire-and-forget、pipeline 部分失败（`ignore_errors`） | 可选收益，不做也行 |

### 2.3 本仓影响面（实测）

| 模式 | 处数 | 文件数 |
|---|---|---|
| `cmd(` / `query_async` | 22 | 3 |
| `Client::open` | 2 | 2 |
| `MultiplexedConnection` / `ConnectionManager` 引用 | 0 | 0（deadpool 封装） |

结论：**redis 这一批比想象的小** —— 最大的 breaking（自定义 `FromRedisValue`）本仓零命中，
剩下是 deadpool 的 `Config`/`Pool` 构造与少量命令调用。

### 2.4 迁移步骤

1. `Cargo.toml`：`redis = "1.7"`、`deadpool-redis = "0.23"`（workspace 段 + 各 crate 已用 `{ workspace = true }`，改一处即可）
2. `cargo update -p redis -p deadpool-redis`
3. 编译 → 按报错改（预计集中在 `deadpool_redis::Config` 与 `Pool::get().await`）
4. `cargo tree -i socket2` 确认只剩 0.6；`cargo tree -d` 看还有没有重复的 redis

### 2.5 执行结果（2026-10-22 01:2x，已在本机跑通）

改了两行 `Cargo.toml`（`redis = "1.7"`、`deadpool-redis = "0.23"`）+ `cargo update`，
编译只报**两类**错，共 **12 处**：

| 处数 | 位置 | 原因 | 改法 |
|---|---|---|---|
| 2 | `synapse-common/src/task_queue.rs:353,449` | `redis::from_redis_value` 收**所有权 `Value`**（1.0 的 breaking） | 353 行把 payload 从 `stream_id.map` **`remove` 出来**（不 clone，消费热路径）；449 行去掉 `&` |
| 10 | `synapse-cache/src/{error.rs,remote.rs}` | `ErrorKind::IoError` → **`ErrorKind::Io`**（变体改名，本仓此前没在文档里记这条） | 直接改名 |

lock 变化：`redis 0.29.5 → 1.7.1`、`deadpool-redis 0.20.0 → 0.23.1`（带进 `deadpool 0.13.1` /
`deadpool-runtime 0.3.1` / `arcstr` / `xxhash-rust`），**`socket2 0.5.10` 被移除** ——
`cargo tree -i socket2` 现在只剩 `socket2 v0.6.5`（经 `dns-lookup`），用户要的"顺带消 socket2 0.5"达成。

验证：`cargo check --workspace --all-features` 通过；
`cargo clippy --workspace --all-targets --features test-utils --all-features -- -D warnings` **exit 0**；
`cargo nextest run -p synapse-cache --all-features` **93/93 通过**（含连真 redis 的
pub/sub、EVAL、MGET、TTL 用例）。

遗留（非本批）：`cargo tree -d` 里 `deadpool 0.12.3` 与 `0.13.1` 并存，但 0.12.3 的唯一来源是
**`wiremock 0.6.5`（dev-dependency，测试用 HTTP mock）**，不在生产树里 —— 不动。
5. 跑 redis/cache 相关集成子集 + `cargo deny check`

## 3. CI 双跑切默认

`2575dfa6f`（*"ci(test-db): 集成测试双跑 schema/database 两种隔离（P3）"*）
**已在 HEAD 祖先链**（`git merge-base --is-ancestor 2575dfa6f HEAD` 实测通过）。

缺口只剩两件：

1. **首轮 CI 结果**：需要 push 之后看 `Test & Lint` 两个隔离车道是否都绿（`ci.yml` 的
   `rust-toolchain × features` 矩阵之外，测试任务本身也分了隔离方式——以 `2575dfa6f` 的改动为准）；
2. **§5.4 的首次 `DROP DATABASE` 复测**：设计文档记的是「首轮满负载后第一次 DROP **26.4 s**」，
   你已把 `CHECKPOINT` 加进模板构建，理论应回到 ~0.3 s。

复测方法（待执行）：

```bash
bash scripts/test_db_template.sh ensure                  # 带 CHECKPOINT 重建模板
TEST_DB_ISOLATION=database TEST_DATABASE_URL=… # 跑一批集成测试
# 记录第一个 DROP DATABASE 的耗时（日志或 pg_stat_statements）
```

**本轮试测了，但结果不可用**（如实记录，别当成结论）：

| 动作 | 实测 | 说明 |
|---|---|---|
| 模板状态 | `synapse_tmpl_v2_1bcc18cd9cc9e519`，20 MB，`datallowconn=false` | 模板已在 **55432** 集群（不是 5432），CHECKPOINT 在 `test_db_template.sh:122` |
| `CREATE DATABASE … TEMPLATE` ×3 | 0.59 s / 1.20 s / 1.18 s | 设计文档记的是 0.29–0.79 s |
| 干净克隆立即 `DROP DATABASE` | **19.8 s** | 与 CHECKPOINT 后的预期（0.3 s）不符 |
| 写脏（~40 MB）后先 `CHECKPOINT` 再 `DROP` | 0.18 s | —— |

**为什么不可用**：测的同时本机 **load average 31.85 / 32.81 / 27.12**（Mac mini），
另一会话在跑测试与构建。`DROP DATABASE` 是纯 IO 动作，19.8 s 删一个 20 MB 库在这种负载下
只反映 IO 争抢，不反映 CHECKPOINT 是否生效。上表只能说明"**当时**很慢"，不能说明"CHECKPOINT 没用"。

**要在静默窗口重测**（这才是 §3 的验收）：

```bash
uptime                                    # 先确认 load 接近核数以下
bash scripts/test_db_template.sh ensure   # 幂等，已含 CHECKPOINT
# 克隆 3 次记 CREATE 耗时；干净克隆立即 DROP 记 1 次；写 40 MB 后再 DROP 记 1 次
```

顺带一条**待验的口径**：设计文档把 26.4 s 归因于"迁移写脏的 shared_buffers 没人刷"，
所以 CHECKPOINT 应该有效。但本轮"干净克隆立即 DROP"比"写脏后 DROP"**慢两个数量级**，
方向与上述归因相反 —— 静默窗口重测时务必把这两组分开测，否则会把 IO 争抢误读成机制问题。

### 3.1 静默窗口复测（2026-10-22 04:44，**结果可用**）

前置条件全部核过：**10 核**、`pgrep -fl "cargo|rustc|nextest"` 无命中、
55432 集群 `pg_stat_activity` 计数为 **0**、`load average` 7.07（下降中）。

| 动作 | 实测 | 对比设计文档 |
|---|---|---|
| `CREATE DATABASE … TEMPLATE`（冷 / 热 / 热） | **612 ms / 157 ms / 134 ms** | 0.29–0.79 s ⇒ 同量级，热路径更快 |
| 干净克隆**立即** `DROP DATABASE`（3 次） | **3.09 s**（冷）/ **36.8 ms** / **30.8 ms** | 26.4 s ⇒ **不复现** |
| 写脏 ~40 MB 后**不** CHECKPOINT 直接 DROP | 43.6 ms | — |
| 写脏 ~40 MB → `CHECKPOINT` → DROP | 129.9 ms | — |

**结论**：

+ **26.4 s 的主要成分是负载，不是"没 CHECKPOINT"**。同一台机器、同一模板，空闲时
  DROP 是 **30–130 ms**；之前那次 19.8 s 与本次 3.09 s 的冷启动都远高于热路径，
  说明剩余方差来自文件系统冷缓存，而不是 shared_buffers 里没刷的脏页。
+ 写脏后再 `CHECKPOINT` 反而**更慢**（130 ms vs 44 ms）—— 因为 CHECKPOINT 自己要把
  40 MB 刷盘，而 DROP 只需删掉文件。所以模板构建里的 `CHECKPOINT`（`test_db_template.sh:122`）
  对**克隆出来的**库意义有限；它的价值在**模板自身**：保证模板里没有未刷的脏页，
  从而让克隆（`CREATE DATABASE … TEMPLATE` 是文件级拷贝）拿到一致的、不触发额外刷盘的状态。
+ 因此 §3 的验收标准应写成"**空闲机器上克隆 ≤ 0.2 s、DROP ≤ 0.15 s**"，
  而不是拿 26.4 s 当基线去比。

顺带一条运维观察：55432 上现有 **4 个** `synapse_tmpl_v2_*` 模板（各 20 MB，
`datallowconn=false`），是不同指纹下 `ensure` 反复生成的残留 —— 归 janitor
（`test_db_template.sh` 的 prune 路径）管，本轮未动，避免撞上其它会话正在用的那个。

## 4. A3 v1 路由退役（112 处 / 79 路径）

**当前不能动。** 工作区正被另一会话的契约链重生成占用：`scripts/contract/extract_registered.py`、
`gen_route_handlers.py`、`docs/synapse-rust/ROUTE_HANDLERS.json`、`ROUTE_CONTRACT.md`、
派生表都在改。这时改路由字面量 = 两边都红（契约链是单向派生，谁先提交都会覆盖另一方）。

建议分批（每批一次提交 + 跑 `bash scripts/contract/check_route_contract.sh`）：

| 批 | 内容 | 前置 |
|---|---|---|
| 1 | 纯 v1 组（无 v2 孪生，112 处里先挑这类） | 契约链那批先落地 |
| 2 | v2 孪生组 | 确认 SDK 侧已全部走 v2（`check_sdk_route_coverage.py` 绿） |
| 3 | 文档/清单收尾（`ROUTE_CONTRACT.md`、SDK pin、路由计数） | 1、2 完成 |

## 5. L-7 全量套件

实测锚点：admin 子集 110 条 = **709 s**（4 线程，≈6.4 s/条）。全量 1619 条线性外推 **2–2.5 h**，
与文档自估的 ~2 h 一致。

夜间跑法（**已固化成脚本，不要手敲**）：

```bash
bash scripts/run_full_integration.sh                       # 全量，约 90 min
JOBS=1 bash scripts/run_full_integration.sh                # PG 锁表打爆时降并发
bash scripts/run_full_integration.sh -E 'test(api_admin_)'  # 透传给 nextest（别加 `--`）
bash scripts/run_full_integration.sh --classify /tmp/xxx.log  # 只复核历史日志
```

脚本封住的四件事（都是 2026-10-22 那次手工执行踩出来的）：
`--all-features`（少它出 3 条假红）、`--no-fail-fast`（少它 1500 条根本不跑）、
`-j 2`（8 路并行把 PG 锁表打爆）、以及**库没准备好就拒跑**（`setup_fresh_test_app()`
拿不到库时用例静默 return、nextest 记 PASS ⇒ 假绿）。退出码：0 全绿 / 1 有真失败 /
2 只有基础设施失败 / 3 前置未通过。

**别和别的活并行**：它独占 `target/` 锁与 PG 连接池，并行只会两边都慢（§3 的计时实验同理）。

## 6. 阻塞与残留（2026-10-22 更新）

**已解除**：原先挡住一切的是"另一会话的在飞改动让 preflight 必红"（3 个新 py 脚本没过 `black`）。
对方陆续提交后（`022cbd67f` / `7fae7ad14` 等），提交链恢复，本轮 7 个提交全部通过 preflight。

**仍阻塞**：契约链（`scripts/contract/*`、`ROUTE_HANDLERS.json`、`ROUTE_CONTRACT.md`、
`API_COVERAGE_REPORT.md`、`ci.yml`、`room_summary.rs`、`worker.rs`）**还在对方索引里未提交**
⇒ A3（改路由字面量）动不了，理由与解挡条件见 §7.4。

**一个仍待裁决的口径重叠**：对方新增的 `scripts/ci/check_deny_unknown_fields_ratchet.py` 是
**按文件的 deny 站点计数棘轮**（只许升不许降）；`741f8647f` 提交的
`routes/admin/dto_guard_tests.rs` 是**按 struct 的"deny 或登记"双向断言**。
两者管同一件事的不同侧面（"全局不许减" vs "每个 DTO 必须有决定"），能共存，
但两套登记都要维护 ⇒ 建议以 `dto_guard_tests` 为**主口径**（它带理由登记），
文件级棘轮降级为"总量护栏"。

## 7. 执行结果汇总（2026-10-22）

### 7.1 提交

| 批次 | 提交 | 内容 |
|---|---|---|
| redis | `022cbd67f` | `redis 0.29 → 1.7`、`deadpool-redis 0.20 → 0.23`（详见 §2.5） |
| P-A | `1ced60cd5` | `sha1 0.11` / `sha2 0.11` / `hmac 0.13` / **`hkdf 0.13`（被迫同批）** |
| P-B | `681bdd40a` | `argon2 0.6`（连带 `password-hash 0.6`、`blake2 0.11`，并新增一个口令哈希串解析 crate） |
| P-C | `cb9165f5d` | `aes-gcm 0.11` / `chacha20poly1305 0.11`（`aead 0.6` / hybrid-array） |
| P-D | `f42893bb2` | `aes 0.9` / `ctr 0.10`（SSSS 的 AES-256-CTR） |
| P-E | `615004f7f` | 三族"不动"的理由写进 Cargo.toml；删掉已无用的 `generic-array` |

### 7.2 一个反复踩的坑：**同名不同版本的类型**

`rsa 0.9` 停在旧一代 trait 上，凡与它交互都必须用**它自带的 re-export**：

| 场景 | 错误写法（能看懂但不编译） | 正确写法 |
|---|---|---|
| `SigningKey::<Sha256>::new` 探测 RSA-SHA256 | `sha2::Sha256`（digest 0.11） | `rsa::sha2::Sha256`（digest 0.10） |
| `RsaPrivateKey::new(&mut rng, 2048)` | `aes_gcm::aead::OsRng` / `rand::rngs::OsRng` | `rsa::rand_core::OsRng`（rand_core 0.6） |

同理 `hkdf 0.12` 绑 digest 0.10 的 `Sha256`，与 `sha2 0.11` 混用会在
`Kdf::<sha2::Sha256>::new` 处报 `OutputSizeUser` 不满足 —— 所以 P-A 里 hkdf 只能同批升。

**判据**：升级某一族之后如果报"某个 trait 没实现（`OutputSizeUser` / `FixedOutput` /
`CryptoRngCore`）"，先查那个 API 属于哪个 crate、它依赖的底层是第几代；不要试图
`use` 另一个版本的类型去凑，那会得到"两个同名类型"的报错或静默不匹配。

### 7.3 验收矩阵（全部实测）

| 项 | 命令 | 结果 |
|---|---|---|
| 编译 | `cargo check --workspace --all-targets --all-features` | exit 0（P-A…P-E 每批各一次） |
| 静态检查 | `cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings` | exit 0（每批） |
| hash/mac/kdf 跨版本 | 4 条 KAT（NIST / RFC 4231 / RFC 6238 / argon2 旧版哈希串） | 4/4 升级前后各一次 |
| 口令哈希 | `nextest -p synapse-common --lib -E 'test(crypto::tests) \| test(password_hash_pool)'` | 104/104 |
| AEAD 存量密文 | `legacy_{aes_gcm,xchacha}_ciphertext_still_decrypts` | 2/2（0.10 产物在 0.11 下可解） |
| SSSS 存量数据 | `secret_encrypted_before_the_upgrade_still_decrypts` + NIST CTR 向量 | 22/22 |
| 单元车道 | `nextest --test unit --features test-utils --no-fail-fast` | 1891/1895（4 条失败**全是树上别人的在飞改动**，见 §6） |
| SQL 棘轮 + 占位符扫描 | `nextest --test unit -E 'test(sqlx_ratio_gate) \| test(no_new_production_literal_dynamic_sql) \| test(placeholder_scan_tests)'` | 22/22 |

### 7.4 仍然没做：A3 v1 路由退役

**没有开工，是被外部条件挡住的**，不是漏了：另一会话的工作区里未提交地改着
`scripts/contract/{extract_registered.py,check_route_contract.sh,test_extract_registered.py}`、
`docs/synapse-rust/{ROUTE_HANDLERS.json,ROUTE_CONTRACT.md,API_COVERAGE_REPORT.md}`、
`synapse-web/src/routes/{room_summary.rs,worker.rs}` 与 `.github/workflows/ci.yml`。
A3 要改**路由字面量**，而契约链是"路由源码 → `extract_registered.py` → 三张派生表 →
6 个 fixture → 两份 ledger 快照 → `ROUTE_CONTRACT.md` → `route-table.json`"的单向链；
现在动它等于和对方在**同一批生成物**上互相覆盖。

**解挡条件**：对方把契约链这批提交掉（`git status` 里那些 `M/A` 行消失）。
之后按文档 §4 的分批（纯 v1 组 → v2 孪生组 → 文档收尾），每批跑
`bash scripts/contract/check_route_contract.sh` 复核。

### 7.5 L-7 全量套件

**权威跑（`--test integration --all-features --no-fail-fast`）最终结果**：

| 指标 | 值 |
|---|---|
| 用例总数 | **1637** |
| 通过 | **1630** |
| 失败 | **7** |
| 耗时 | **91.1 分钟**（5466 s，121 条 slow） |
| 日志 | `/tmp/l7_allfeat_full.log` |

7 条失败**没有一条属于本次依赖升级**，分三类：

| 类别 | 条数 | 用例 | 证据 |
|---|---|---|---|
| **基础设施**（PG 锁表耗尽） | 3 | `api_admin_rbac_wire_tests::user_admin_role_is_confined_to_users`、`api_admin_room_lifecycle_tests::{test_admin_backfill_requires_existing_room,test_admin_kick_emits_membership_event}` | 失败点是 `tests/integration/mod.rs:555` 的 `pooled schema acquisition`，报 **`out of shared memory`**（并发克隆 20 MB 模板，222 张表各需加锁）。`-j 1` 串行复跑 **4/4 全绿** |
| **计时抖动** | 1 | `api_beacon_location_tests::…room_backpressure_token_bucket` | token bucket 在慢执行下回填，等不到 429。串行复跑 **PASS**（8.8 s） |
| **HEAD 既有 / 他人在飞** | 3 | `api_appservice_p1_tests::test_appservice_transaction_push`（他人正在改 `/events` 响应形状）；`api_route_snapshots_tests::snapshot_register_uia_401_challenge`（快照最后更新 2026-07-07，此后 `93a2b683a` 给 401 加了标准错误体）；`api_space_routes_tests::test_space_children_hierarchy_suite_…`（`19e906c09` 改过 `children_hierarchy.rs`，`/spaces/room/{id}/parents` 不再返回 `room_id`） | 均与本批改动的文件/调用链无交集 |

**折算有效通过率：1634/1637（99.8%）**，剩余 3 条都有明确归属。

#### 7.5.1 两次跑错口径的教训

+ **第一轮**（`/tmp/l7_full.log`）忘了 `--no-fail-fast`，在 115/1636 处被一条失败中断，
  剩下 1521 条**根本没跑**。全量套件里本来就有别人在飞改动造成的红，fail-fast 会让日志看起来"只跑了一点点"。
+ **第二轮**（`/tmp/l7_full2.log`）用了精简 feature 集
  （`test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications`，**不含 `cas-sso`/`saml-sso`**），
  于是 3 条"假红"：两条 route-ledger manifest 快照 + `snapshot_login_flows_v3`
  （快照里带着 `m.login.cas` / `m.login.sso`，精简集下路由根本没注册）。用 `--all-features` 复跑即转绿。
  **排查这 3 条假红花的时间比它们跑的时间还长** —— 与权威车道同口径是硬要求。

**并发上限（环境）**：8 路并行跑全量时，`CREATE DATABASE … TEMPLATE` 会撞 PG 的
`out of shared memory`（锁表）。要么 `-j 2`，要么把 `max_locks_per_transaction` 调大；
这不是代码问题，但会让"全量跑一遍"的结论被基础设施污染。

### 7.5.2 早先那次跑的记录（保留以便对照）

夜间（04:47）按 §5 的命令启动。第一轮**忘了 `--no-fail-fast`**，在 115/1636 处被一条失败中断；
第二轮补上跑完：**1636 条 / 74.6 分钟 / 1628 通过 / 8 失败**（`/tmp/l7_full2.log`）。

然后把 8 条逐条**单独复跑**，再**用 `--all-features` 复跑**（这才是 §5 之外的权威车道）：

| 用例 | 精简 feature 集 | `--all-features` | 归属 |
|---|---|---|---|
| `api_beacon_location_tests::…room_backpressure_token_bucket` | FAIL（83.7 s） | **PASS** | 负载偶发：token bucket 在慢执行下回填，等不到 429 |
| `api_route_ledger_tests::declared_route_manifest_…default_state` | FAIL | **PASS** | 我跑的时候没带 `cas-sso` / `saml-sso`，路由面少了若干端点 |
| `api_route_ledger_tests::…worker_enabled_state` | FAIL | **PASS** | 同上 |
| `api_route_snapshots_tests::snapshot_login_flows_v3` | FAIL（少 `m.login.cas` / `m.login.sso`） | **PASS** | 同上（快照里带着这两个 flow） |
| `api_appservice_p1_tests::test_appservice_transaction_push` | FAIL | FAIL | **别人的在飞改动**：`/events` 响应改成 `{events,next_token}`，该文件正被改（`M tests/integration/api_appservice_p1_tests.rs`），断言还在按数组取 |
| `api_route_snapshots_tests::snapshot_register_uia_401_challenge` | FAIL | FAIL | **HEAD 既有**：快照最后更新 2026-07-07，而 `93a2b683a` 之后 401 响应多了 `errcode`/`error`，快照没跟 |
| `api_space_routes_tests::test_space_children_hierarchy_suite_…` | FAIL | FAIL | **HEAD 既有**：`children_hierarchy.rs` 在 `19e906c09`（2026-10-21）刚被改过，`/spaces/room/{id}/parents` 返回的 `SpaceResponse` 不再带 `room_id`（实测 `Null`），测试期望 `!2jb…` |
| `sync_service_tests_migrated::test_incremental_lazy_load_…_state_delta_members` | FAIL | FAIL | **未归属**，需 owner 看：断言是"limited timeline 不应重放 timeline 之外的 `m.room.member` 状态差异"，与本次依赖升级无调用关系 |

**教训（写下来免得再犯）**：跑全量套件必须与权威车道同口径
（`--test integration --all-features`）。带精简 feature 集跑出来的 3 条"失败"纯属自伤，
排查它们花的时间比跑一遍还长。另外 `--no-fail-fast` 也是必须的 —— 全量套件里
本来就有别人在飞改动造成的红，fail-fast 会让剩下 1500 条根本没跑。
