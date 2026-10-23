# 测试失败分诊（2026-10-23，HEAD `c878fe12e`）

## 一句话

仓库里有 **12 条失败**，但它们**不是同一类问题**：2 条是测试自身写错（已修），
8 条来自其他人尚未提交的改动，2 条是 2026-10-22 就记录过的「HEAD 既有契约红」。
**把 12 条当成 12 个 bug 去逐个修，会有一半是白干或者冲突。**

## 已修（本次）

| 测试 | 原因 | 处理 |
|---|---|---|
| `api_key_backup_route_table_tests::key_backup_routes_are_wired_under_v1_and_r0` | **与 `_under_v3` 完全重复**，且 v1 部分在断言**不存在**的路由 | **删除**（v3 面已由 `_under_v3` 逐条覆盖 8 条路径） |
| `test_ddl_guard_tests::no_unallowlisted_self_built_schema_in_tests_dir` | `registration_service_tests_migrated.rs:189` 的 `ALTER TABLE … CHECK (false)` 是**故障注入**，未登记 | **补进 `scripts/ci/test_ddl_allowlist_tests_dir` 的 (a) 组** |

### 为什么删那条测试不是"降低覆盖"

`key_backup_routes_are_wired_under_v3` 的注释自己写着：

> All three nested prefixes (v1/r0/v3) share the same inner `Router`; verifying v3 is
> sufficient to prove `create_key_backup_router` is participating in the merge.

而事实与这个名字/检查方向都不符：

- `create_key_backup_router` 结尾只有 `.nest("/_matrix/client/v3", router)`；
- 全仓 **0 处** `/_matrix/client/r0` 路由；
- 三个 v1 基线（`v1_surface_baseline` / `v1_no_twin_baseline.txt` / `v1_spec_keep_baseline.txt`）
  里都**没有** `room_keys` —— `/room_keys/*` 从来没在 v1/r0 面注册过。

也就是说这条断言的期望值与代码事实相反，它从 `2026-10-22` 起就是红的
（`COMPREHENSIVE_AUDIT_2026-10-22.md:1422` 已记为「HEAD 既有」）。

## 未修：其他人尚未提交的改动（8 条）

| 测试 | 直接原因 | 归属证据 |
|---|---|---|
| `api_route_ledger_tests::declared_route_manifest_full_snapshot_matches_default_state` | 路由快照 drift（SAML/CAS） | 主树 `scripts/contract/{ledger_annotations.txt,ROUTE_HANDLERS.json,ROUTE_CONTRACT.md}` 与 `synapse-web/src/routes/*.rs` 全在未提交状态 |
| `api_route_ledger_tests::…_worker_enabled_state` | 同上 | 同上 |
| `sqlx_ratio_gate_tests` ×4（含 `passes_on_current_tree`） | 棘轮实时测量越界：生产动态 SQL **133→136**、测试动态 **767→779**、静态 **1367→1360** | 这 15 处不在本次任何改动里；基线文件最后更新是 `1ced60cd5` |
| `sqlx_dynamic_literal_guard_tests::no_new_production_literal_dynamic_sql` | 同上（同一份 census） | 同上 |

**处理**：不修。修它们要么改别人正在写的代码，要么上调 `sqlx_dynamic_ratio_baseline`
（那是把棘轮拆掉——基线自己写着「每一条都是待还的债」）。等主树干净后由改动人自己补。

## 未修：HEAD 既有契约红（2 条）

| 测试 | 说明 |
|---|---|
| `api_placeholder_contract_p1p2_tests::test_power_levels_contract_rejects_non_members` | `COMPREHENSIVE_AUDIT_2026-10-22.md:1579` 明确记为「HEAD 既有红，本批未触及对应 handler」 |
| `api_relations_authorization_tests::relations_event_type_route_filters_over_http` | 同上 |

这两条**不是测试写错**，是 handler 的占位/契约行为没实现。修它属于实现功能，不是修测试。

## 未修且未归类（1 条，需要单独诊断）

`api_beacon_location_tests::test_beacon_location_applies_room_backpressure_token_bucket`

- 耗时 **43.9 s**（全仓最慢的一条）
- 测试体里创建 40 个参与者后再触发桶限流
- 未确认是「真 bug」还是「阈值/时序脆弱」。**没有 DB 空闲窗口做二分定位，本次不下结论。**

## 怎么复现本报告的数字

```bash
# 全量 integration（7 分钟，先 `bash scripts/test_db_template.sh ensure`）
env -u TEST_DB_TEMPLATE_SCHEMA TEST_DB_ISOLATION=database \
  cargo nextest run --test integration --all-features --test-threads 4 --no-fail-fast

# unit 那 6 条
env -u TEST_DB_TEMPLATE_SCHEMA \
  cargo nextest run --test unit --features test-utils --test-threads 4 --no-fail-fast \
  -E 'test(sqlx_ratio_gate) + test(sqlx_dynamic_literal) + test(no_unallowlisted_self_built)'

# sqlx 棘轮的实时数字（不跑测试也能看）
bash scripts/ci/check_sqlx_dynamic_ratio.sh
```

## 未覆盖

- 「其他人尚未提交的改动」这个归属判断，依据是 `git status` 的脏文件集合 + 基线文件的
  `git log`。**没有**逐个 commit 追溯那 15 处动态 SQL 的引入点。
- beacon 那条没有诊断结论。
- 只在 `tests/integration` 与 `--test unit` 两个车道上核对；`--test e2e`（需 `--ignored`）
  与 `tests/performance/` 没跑。
