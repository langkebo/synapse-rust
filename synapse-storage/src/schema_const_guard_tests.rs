//! D14-2 守卫：**共享列清单常量 ↔ 当前 schema**。
//!
//! ## 为什么需要它
//!
//! §8.6 把 D-14 的 29 处运行期拼装按"插进去的是**值**还是**标识符**"分档：值那 3 处已在 C64
//! 回收，剩下 26 处里有 **16 处**（②）是把 `ROOM_EVENT_COLS` / `STATE_EVENT_*_COLS` /
//! `STATE_GROUP_STATE*_COLS` 这类**共享列清单常量**用 `format!` 拼进 SQL 文本。宏要求调用点字面量
//! （R1），所以这些站点只能保持动态 —— 而把它们内联成字面量等于把一份列清单复制成多份
//! （违反铁律 2，且 `ROOM_EVENT_COLS` 的 doc comment 与 pagination bench 都依赖"只有一份"）。
//! ⇒ 换一条路：**用测试期守卫替代编译期检查**。
//!
//! 常量漂移的真实后果是**运行期 42703**（列被改名/删除后，只有执行到那条查询才会炸）—— 这正是
//! 本战役在别处挖出硬缺陷的形态。本模块把它提前到测试期：对每条常量，在真 baseline 的当前 schema
//! 上 `describe("SELECT <cols> FROM <基表> WHERE FALSE")`，断言两件事：
//!
//! ① 整段列清单**能被 PG prepare**（任一列名/表达式漂移 ⇒ 42703 ⇒ 红）；
//! ② **输出列名逐一等于消费结构体的字段名**（`#[sqlx(rename)]` 生效后的目标名）—— 动态路径走
//!    `FromRow`，它是**按列名**映射的，所以别名写错（例如把 `processed_at` 写成 `processed_ts`，
//!    见 C38/D-19）同样只会在运行期炸。
//!
//! ## 边界（写清楚，免得被当成"万能守卫"）
//!
//! - 它**不**执行真实谓词与连接（那是各调用点自己的真 baseline 往返的事），只验证列清单本身；
//! - ③ 那一档（`ORDER BY` 列名 + 方向、rank 子表达式）**没有常量可守** —— 片段就地写在 `format!`
//!   里，它的守卫是各方法自己的真 baseline 往返：`space/db_tests.rs`（`search_spaces`）、
//!   `user/db_tests.rs`（`search_users` / `search_users_with_presence`）、以及
//!   `membership/mod.rs` 的 `test_get_room_members_paginated_with_profiles_covers_every_query_shape`
//!   （C65-0 补的，8 种形态全覆盖）；
//! - ④ `VACUUM`/`REINDEX` 的表名/索引名是**运行期入参**，没有可枚举的期望值。

use sqlx::{Column, Executor};

/// 一条守卫用例：常量文本 + 它被拼进 SQL 时的基表 + 必须出现的输出列名。
struct ColumnConstCase {
    /// 常量名（断言消息里指名道姓，失败时不必再去数第几列）。
    const_name: &'static str,
    /// 常量文本（共享列清单）。
    columns: &'static str,
    /// 该常量拼进的基表。
    table: &'static str,
    /// 期望的输出列名 = 消费结构体的字段名（`#[sqlx(rename)]` 生效后的目标名）。
    expected_columns: &'static [&'static str],
}

fn column_const_cases() -> Vec<ColumnConstCase> {
    vec![
        ColumnConstCase {
            const_name: "ROOM_EVENT_COLS",
            columns: crate::event::ROOM_EVENT_COLS,
            table: "events",
            // `RoomEvent` 的 14 个字段。⚠️ 第 9 列必须叫 `processed_at`：字段名是 `processed_ts`，
            // 靠 `#[sqlx(rename = "processed_at")]` 对上（C38/D-19 的实测）。
            expected_columns: &[
                "event_id",
                "room_id",
                "user_id",
                "event_type",
                "content",
                "state_key",
                "depth",
                "origin_server_ts",
                "processed_at",
                "not_before",
                "status",
                "origin",
                "stream_ordering",
                "redacts",
            ],
        },
        ColumnConstCase {
            const_name: "STATE_EVENT_OUTER_COLS",
            columns: crate::event::state::STATE_EVENT_OUTER_COLS,
            table: "events",
            // `StateEvent` 的 20 个字段（同样有 `processed_ts` ↔ `processed_at` 的 rename）。
            expected_columns: &[
                "event_id",
                "room_id",
                "sender",
                "event_type",
                "content",
                "state_key",
                "unsigned",
                "is_redacted",
                "origin_server_ts",
                "depth",
                "processed_at",
                "not_before",
                "status",
                "origin",
                "user_id",
                "stream_ordering",
                "prev_events",
                "auth_events",
                "signatures",
                "hashes",
            ],
        },
        ColumnConstCase {
            const_name: "STATE_EVENT_INNER_COLS",
            columns: crate::event::state::STATE_EVENT_INNER_COLS,
            table: "events",
            // 内层子查询用的原始列清单：与 OUTER 同形但**没有** `processed_at`
            // （OUTER 用 `NULL::BIGINT as processed_at` 补上）。
            expected_columns: &[
                "event_id",
                "room_id",
                "sender",
                "event_type",
                "content",
                "state_key",
                "unsigned",
                "is_redacted",
                "origin_server_ts",
                "depth",
                "not_before",
                "status",
                "origin",
                "user_id",
                "stream_ordering",
                "prev_events",
                "auth_events",
                "signatures",
                "hashes",
            ],
        },
        ColumnConstCase {
            const_name: "STATE_GROUP_STATE_COLS",
            columns: crate::state_groups::STATE_GROUP_STATE_COLS,
            table: "state_group_state",
            expected_columns: &["state_group_id", "event_type", "state_key", "event_id"],
        },
        ColumnConstCase {
            const_name: "STATE_GROUP_STATE_INNER_COLS",
            columns: crate::state_groups::STATE_GROUP_STATE_INNER_COLS,
            table: "state_group_state",
            // 该常量喂给 `Vec<(String, String, String)>`（元组按**位置**映射）⇒ 列名与顺序都要一致。
            expected_columns: &["event_type", "state_key", "event_id"],
        },
    ]
}

#[tokio::test]
async fn shared_column_constants_match_the_current_schema() {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated test pool");
    let pool = isolated.pool();
    let mut conn = pool.acquire().await.expect("acquire a connection");

    // ⚠️ 失败**累积**后一次断言（而不是每条 `panic!`/`unwrap`）：本仓 clippy 以
    // `-D clippy::panic` 阻断"生产代码里的 panic"，而 `#[cfg(test)]` **模块文件**在 clippy 眼里
    // 仍属 lib 代码 ⇒ 这里不能用 `panic!`。累积还有一个好处：常量漂移时一次跑出**全部**不一致项，
    // 不用改一条跑一次。
    let mut failures: Vec<String> = Vec::new();

    for case in column_const_cases() {
        // R9：测试区探针一律动态 SQL（宏的条目不会被 `cargo sqlx prepare` 收集，
        // 离线 `--all-targets` 会 E0282）。`WHERE FALSE` 让 describe 不返回任何行。
        let sql = format!("SELECT {} FROM {} WHERE FALSE", case.columns, case.table);
        match (&mut *conn).describe(sql.as_str()).await {
            Ok(described) => {
                let actual: Vec<&str> = described.columns().iter().map(|column| column.name()).collect();
                if actual != case.expected_columns {
                    failures.push(format!(
                        "{}: 输出列名必须逐一等于消费结构体的字段名（动态路径走 `FromRow`，按**列名**映射，\
                         别名写错只会在运行期炸）\n  实际: {actual:?}\n  期望: {:?}",
                        case.const_name, case.expected_columns
                    ));
                }
            }
            Err(error) => failures.push(format!(
                "{}: `SELECT … FROM {}` 在当前 schema 上无法 prepare —— 列名/表达式已漂移\
                 （运行期后果是 42703）: {error}",
                case.const_name, case.table
            )),
        }
    }

    assert!(
        failures.is_empty(),
        "共享列清单常量与当前 schema 不一致（{} 项）：\n{}",
        failures.len(),
        failures.join("\n")
    );
}
