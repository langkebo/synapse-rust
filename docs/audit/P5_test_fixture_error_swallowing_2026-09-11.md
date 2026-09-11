# 测试夹具吞掉数据库写入错误（CLAUDE.md 同型反模式）

> **日期**: 2026-09-11
> **基线提交**: `496df47a`
> **范围**: P5「工程质量」；与 `CLAUDE.md` 明令的 `unwrap_or_default` 反模式同源

---

## 0. 结论

`synapse-storage` 的测试夹具里有 **32 处**把数据库写入结果直接丢弃：

```rust
sqlx::query("INSERT INTO rooms (...) VALUES (...)")
    .bind(room_id)
    .execute(pool)
    .await
    .ok();                    // ← 错误被丢弃
```

`CLAUDE.md` 对此有明确规则：

> **Never `unwrap_or_default()` on DB queries in security-relevant paths** —
> it silently converts DB errors to "empty/false" defaults.

测试夹具里的 `.ok()` 是同一问题的更隐蔽变体：**吞掉的是 setup 错误**。
夹具静默不生效 → 测试带着缺失的前置条件继续跑 → 真正的失败在**几条语句之后
以完全无关的形式**爆出。

### 实测代价（本次审查第 26–27 轮）

`room_summary::db_tests::ensure_test_room` 丢弃了 `INSERT INTO rooms` 的结果。
随后测试失败于：

```
room_summary_members violates foreign key constraint fk_room_summary_members_room
Key (room_id)=(...) is not present in table "rooms"
```

这个报错指向**成员插入**，而真正出错的是**之前的房间插入**。
真实原因被 `.ok()` 挡住，消耗了相当长的排查时间。

---

## 1. 处理

### 1.1 防复发守卫（`tests/unit/test_fixture_error_handling_tests.rs`）

扫描范围**刻意限定**在无歧义的测试支持文件：
`*db_tests*.rs`、`test_mocks/*`、`tests/` 下的 `_tests.rs`。

**生产代码不在范围内**：那 113 处 `.ok()` 大多是正当用法
（`Option` 提取、best-effort 缓存写入、`shutdown_rx.recv().await.ok()`）。
把它们一并标记会让守卫充满噪声并最终被关掉 —— 与本项目其他门禁的教训一致。

守卫匹配两种写法：

* `....execute(pool).await.ok();`
* `let _ = sqlx::query(...).execute(pool).await;`

并有一条 `guard_scans_test_support_files` 断言扫描确实覆盖了 ≥10 个文件，
避免守卫本身空转。

### 1.2 修复 32 处

改为 `.expect("test fixture: <op> must succeed — a swallowed error here surfaces
later as an unrelated failure")`，让失败在**发生处**点名自己。

涉及 6 个文件：

| 文件 | 处数 |
|---|---|
| `friend_room/db_tests.rs` | 12 |
| `media_quota/db_tests.rs` | 6 |
| `thread/db_tests.rs` | 5 |
| `saml/db_tests.rs` | 5 |
| `registration_token/db_tests.rs` | 3 |
| `server_notification/db_tests.rs` | 1 |
| `room_summary/db_tests.rs` | 2（手工修，见 §0 的案例） |

> 改造过程中的一个自身失误值得记录：第一版脚本会重建整条语句，
> 结果把单行写法（`sqlx::query(...).execute(pool).await.ok();`）
> 的语句体删掉、只留下 `.expect(...)`，编译直接失败。
> 已 `git checkout` 还原并改为**只替换行尾的 `.ok();`**——
> 不需要重建语句，风险低得多。第二次执行后编译通过。

---

## 2. 验证

```console
# 守卫：RED（改动前，列出全部 32 处） → GREEN（改动后）
$ cargo nextest run --profile test --features test-utils --test unit -E 'test(/test_fixture_error_handling_tests/)'
    PASS guard_scans_test_support_files
    PASS test_fixtures_do_not_swallow_database_writes
    Summary 2 tests run: 2 passed

# 被改造夹具的运行时行为未变
$ cargo nextest run -p synapse-storage --lib \
    -E 'test(/friend_room::db_tests|registration_token::db_tests|thread::db_tests|saml::db_tests|server_notification::db_tests/)' --test-threads 4
    Summary [0.713s] 58 tests run: 58 passed

# 全量门禁
$ ./scripts/check_fmt_ratchet.sh                  # OK: fmt debt at baseline (0)
$ cargo clippy --workspace --all-targets --all-features --locked
CLIPPY_EXIT=0 errors=0 warnings=15                # 15 = 既有基线
$ cargo nextest run --profile test --features test-utils --lib --test unit
Summary 2522 tests run: 2522 passed (1 slow), 4 skipped
```

> `synapse-storage` 另有 2 条 `ensure_test_room` / `ensure_test_event`
> never-used 警告（`voice.rs`），已核对**属于既有 clippy 基线**，与本次改动无关。

---

## 3. 未解决：`room_summary::db_tests::test_add_member_creates_record`

修掉吞错后暴露出的真实顺序更清楚了，但**该用例仍然失败**，且原因尚未定位：

* `ensure_test_room` 的 `INSERT INTO rooms` **确实成功**（改成 `.expect` 后不再 panic）；
* 手工在 `public` 上执行完全相同的语句序列（rooms → users → members）**成功**；
* 但测试里 `add_member` 仍报 room 不存在。

已排除：夹具吞错、schema 缺表、FK 约束缺失、并发放置。
**尚未定位根因**，需单独排查（下一步建议：在测试中直接查询
`SELECT count(*) FROM rooms WHERE room_id = $1` 来确认插入与读取是否落在同一
search_path）。

本轮**未**把它标记为已修复，也未改动其断言。

---

## 4. 剩余同类项（未做）

| 类别 | 数量 | 说明 |
|---|---|---|
| 生产代码 `.ok()` | 113 | 多为正当用法，需逐个判断，不宜机械替换 |
| 生产代码 `let _ = <query/execute>` | 83 | 同上 |
| 其他 crate 的测试夹具 | 未统计 | 本次只覆盖 `synapse-storage`（守卫的扫描范围已含全部 crate，会持续拦截新增） |

守卫已就位，因此**新增**的夹具吞错会被拦住；存量则按需逐个处理（每条都需要
判断"这个失败是否真的可以忽略"，机械替换有把"刻意 best-effort"变成硬失败的风险）。

---

## 5. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 守卫（当前应通过）
cargo nextest run --profile test --features test-utils --test unit \
  -E 'test(/test_fixture_error_handling_tests/)'

# 列出当前仍存在的夹具吞错（应为空）
grep -rn "\.execute(.*)\.await\.ok()" --include='*db_tests*.rs' synapse-storage/src/ || echo "(none)"

# 该用例的未定位失败
export TEST_DATABASE_URL='postgresql://synapse:<pw>@localhost:5432/synapse'
cargo nextest run -p synapse-storage --lib \
  -E 'test(/room_summary::db_tests::test_add_member_creates_record/)' --test-threads 1
```
