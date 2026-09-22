//! 守卫：`ORDER BY <毫秒时间戳>` 必须带决胜键（逐文件计数棘轮）。
//!
//! 为什么需要它（2026-09-22 实测）：`refresh_token::get_rotations` 曾是
//! `ORDER BY rotated_ts DESC`，而 `rotated_ts` 是 **BIGINT 毫秒**。同一毫秒内的两次
//! 轮换并列时 PostgreSQL 可任意顺序返回 —— "最近的在前"这条契约在并列时不确定；
//! 小表顺序扫描下通常返回插入顺序，于是**最旧的排在最前面**。按 CI 口径跑覆盖率时
//! `test_db_record_rotation_and_get_rotations` 正是这么失败的
//! （`left: "new_hash_1" / right: "new_hash_2"`）。
//!
//! 时间戳不是唯一键 ⇒ 按它排序必须再带一个唯一/单调的决胜键
//! （`, id DESC` / `, stream_ordering ASC` / `, event_id DESC` …）。
//! 全仓有 100+ 处历史站点，一次全改风险过大（尤其 `event/*` 的 keyset 分页语义），
//! 因此这里用**逐文件计数棘轮**：已知站点记进
//! `scripts/ci/ts_order_single_key_baseline`，新增会红、修好要收紧基线。
//!
//! **红证明**：往任意被扫描的 `.rs` 里插一行 `-- ORDER BY foo_ts DESC`（单键）→
//! 本测试 FAILED；把基线里某文件计数减 1 → 也 FAILED（基线过期）；恢复 → PASS。

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run_checker(args: &[&str]) -> (i32, String) {
    let out = Command::new("python3")
        .arg("scripts/ci/check_ts_order_tiebreak.py")
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("python3 必须可运行（本守卫的前提）");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// 当前树必须与基线一致（无新增单键排序、无过期条目）。
#[test]
fn timestamp_ordering_tiebreak_ratchet_passes_on_current_tree() {
    let (code, out) = run_checker(&[]);
    assert_eq!(
        code, 0,
        "`ORDER BY <*_ts>` 单键站点必须与 scripts/ci/ts_order_single_key_baseline 一致。\
         新增了不带决胜键的排序时：加第二键（`, id DESC` / `, stream_ordering ASC` …），\
         而不是把它记进基线。输出：\n{out}"
    );
}

/// 扫描本身必须非空转（否则"通过"什么都不证明），且 2026-09-22 修过的那批文件
/// 必须已经从基线里消失 —— 这是那次修复的常驻回归。
#[test]
fn timestamp_ordering_scan_is_not_vacuous_and_the_fixed_files_stay_fixed() {
    let (code, out) = run_checker(&["--print"]);
    assert_eq!(code, 0, "`--print` 必须 exit 0：\n{out}");
    assert!(
        out.contains("单键站点合计：") && !out.contains("单键站点合计：0 处"),
        "扫描必须真的找到站点（非空转）：\n{out}"
    );

    let baseline = fs::read_to_string(repo_root().join("scripts/ci/ts_order_single_key_baseline"))
        .expect("read scripts/ci/ts_order_single_key_baseline");
    for fixed in [
        "synapse-storage/src/beacon.rs",
        "synapse-storage/src/admin_media.rs",
        "synapse-storage/src/captcha.rs",
        "synapse-storage/src/cas/repository.rs",
        "synapse-storage/src/dehydrated_device.rs",
        "synapse-storage/src/device/mod.rs",
    ] {
        assert!(
            !baseline.lines().any(|l| l.starts_with(fixed)),
            "`{fixed}` 在 2026-09-22 已补齐决胜键，不应再出现在单键基线里\
             （若它重新出现，说明修复被回退或被新增站点覆盖）"
        );
    }
}
