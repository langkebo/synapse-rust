# 第二批工程改造：评估与执行顺序（2026-10-21）

> 本文件回答"第二批 5 项能不能做、先做什么、各会自动碰哪些东西"。
> 所有版本号都是 **2026-10-21 用 crates.io API 实测**，不是记忆；影响面是 `*.rs` 实测计数
> （排除 `target/`、`vendor/`、`.worktrees/`）。
> **现状：本文件只有第 3 项（CI 双跑）的前置已完成，其余都还没动手** —— 原因见 §6。

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

夜间跑法：

```bash
TEST_DATABASE_URL="postgresql://synapse:$(bash scripts/lib/db_credentials.sh password)@localhost:5432/synapse_test" \
  cargo nextest run --test integration \
  --features "test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications" \
  --no-fail-fast > /tmp/full_suite_$(date +%H%M).log 2>&1
```

**别和别的活并行**：它独占 `target/` 锁与 PG 连接池，并行只会两边都慢（§3 的计时实验同理）。

## 6. 阻塞与建议顺序

当前真正挡住一切的是一件事：**工作区被另一会话占着**。未提交的 `ci.yml`、`docker/db_migrate.sh`、
`scripts/contract/*`、`ROUTE_HANDLERS.json`、`route_summary.rs`、`worker.rs` 等，
其中 3 个新 py 脚本还没过 `black` ⇒ preflight 必红 ⇒ 我这边**任何提交都进不去**（也不该 `--no-verify`）。

顺带一个需要裁决的重叠：对方新增的 `scripts/ci/check_deny_unknown_fields_ratchet.py` 是
**按文件的 deny 站点计数棘轮**（只许升不许降），我待提交的 `routes/admin/dto_guard_tests.rs` 是
**按 struct 的"deny 或登记"双向断言**。两者管的是同一件事的不同侧面（一个是"全局不许减"，
一个是"每个 DTO 必须有决定"），可以共存，但**得先定谁是主口径**，否则两套登记都要维护。

建议顺序（全程串行，不要并行）：

1. 解开工作区占用：对方提交契约链 + CI 批；我的 P2-C 紧随其后（已就绪，等 preflight 能过）
2. RustCrypto **P-A → P-B → P-C → P-D**（P-C 前先补/确认 KAT）
3. redis 0.29 → 1.x（独立一批，改 `Cargo.toml` 一处 + lock）
4. A3 v1 路由退役分批（1 → 2 → 3），每批都跑契约 gate
5. 最后：夜间起 L-7 全量套件（跑完再复测 §5.4 的首次 DROP，那时数据库最"脏"、数据最真实）
