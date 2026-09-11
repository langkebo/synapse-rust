# 测试隔离 schema 泄漏（100%）与修复

> **日期**: 2026-09-11
> **范围**: P4「资源管理」；`synapse-storage/src/test_isolation.rs`
> **关联**: CLAUDE.md 记录的"1363 个残留 schema"历史问题

---

## 0. 结论

`IsolatedTestPool::drop` **100% 泄漏**它创建的隔离 schema。实测：

| 步骤 | 测试数 | schema 增量 |
|---|---|---|
| 修复前 | 24 | **+24**（100%） |
| 修复后 | 24 | **+0** |

本地原生 Postgres 已累积 **22,532** 个 `test_*` schema。

---

## 1. 根因：两种"fire-and-forget"清理都被进程退出杀掉

`Drop::drop` 是同步的、不能 await，因此清理必须委托出去。原实现：

```rust
std::thread::spawn(move || {
    let rt = Runtime::new().unwrap();
    rt.block_on(async { /* connect + DROP SCHEMA */ });
});
```

`Drop` 立即返回 → 测试结束 → **nextest 是进程级并行（一个测试一个进程）**
→ 进程退出，新线程还没连上 Postgres 就被终止。

**尝试 2**：改用 `LazyLock<Runtime>` 静态运行时 `spawn`（照搬根集成测试
harness 的 `CLEANUP_RUNTIME` 写法）。仍然 **100% 泄漏** —— 原因是
`LazyLock` 静态量在进程退出时被 drop，而 **drop 一个 Runtime 会取消尚未完成的
异步任务**（不是等待它们），`DROP SCHEMA` 因此从未执行。

> 这也解释了为什么该问题长期存在：清理代码"看起来写对了"，
> 日志里也只有 `debug!` 级别的成功记录，泄漏没有可见信号。

---

## 2. 修复：spawn + **join**

```rust
let handle = std::thread::spawn(move || {
    let rt = Runtime::new()?;
    rt.block_on(async { /* connect + DROP SCHEMA IF EXISTS ... CASCADE */ });
});
let _ = handle.join();          // ← 关键：drop 返回前确保 schema 已删除
```

代价是每个隔离测试多一次 connect + DROP（实测该套件总时长
671s → 656s，**没有变慢**，因为并发下这部分开销被掩盖）。收益是不再累积 schema。

另外把 `DROP SCHEMA` 改为 `DROP SCHEMA IF EXISTS`：重复清理或外部已清理时
应为幂等 no-op，而不是报错。

`join()` 的返回值被忽略：清理线程 panic 时不得从 `drop` 中再 panic
（unwinding 期间 panic 会 abort 进程）。

---

## 3. 验证

```console
# 修复前
$ psql ... "SELECT count(*) ... LIKE 'test\_%'"   # 22532
$ cargo nextest run -p synapse-storage --lib -E 'test(/media_quota::db_tests/)' --test-threads 4
    Summary 24 tests run: 24 passed
$ psql ... count                                  # 22556   → +24

# 修复后
$ psql ... count                                  # 22580
$ cargo nextest run ... (同上)
    Summary 24 tests run: 24 passed
$ psql ... count                                  # 22580   → +0
```

门禁：fmt 0、clippy EXIT=0（0 error / 15 = 既有基线）、2522 passed。

---

## 4. 未做：历史残留清理

**22,532 个存量 schema 未清理** —— 删除 2.2 万个 schema 影响面大，
不属于本次任务范围，故先修根因、不动存量。

`scripts/cleanup_test_schemas.sh` 已存在（每个 schema 单独事务，
避免 `out of shared memory`），但其默认连接参数指向
`localhost:15432/synapse_test`，与本机实例（`127.0.0.1:5432/synapse`）不一致；
清理时需用 `PGHOST/PGPORT/PGUSER/PGDATABASE` 覆盖。

**注意**：修复只阻止"继续泄漏"。存量 schema 会继续拖慢 catalog 查询
（`pg_tables`/`pg_indexes` 扫描）、并让 `information_schema` 类查询变慢 ——
建议在方便时清理一次。

---

## 5. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
export PGURL='postgresql://synapse:<pw>@127.0.0.1:5432/synapse'
export DATABASE_URL="$PGURL" TEST_DATABASE_URL="$PGURL"

before=$(psql "$PGURL" -tAc "SELECT count(*) FROM information_schema.schemata WHERE schema_name LIKE 'test\_%';")
cargo nextest run -p synapse-storage --lib -E 'test(/media_quota::db_tests/)' --test-threads 4
sleep 5
after=$(psql "$PGURL" -tAc "SELECT count(*) FROM information_schema.schemata WHERE schema_name LIKE 'test\_%';")
echo "LEAK DELTA: $((after - before))"   # 修复后应为 0

# 存量清理（需先用环境变量覆盖连接参数）
PGHOST=127.0.0.1 PGPORT=5432 PGUSER=synapse PGDATABASE=synapse \
  bash scripts/cleanup_test_schemas.sh
```
