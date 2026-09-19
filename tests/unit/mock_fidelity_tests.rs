//! Mock-fidelity guard (P5 §3.6: "mock 漂移").
//!
//! ## Why this file exists
//!
//! The in-memory stores under `synapse-storage/src/test_mocks/` are used widely
//! by unit and service tests. Where a mock diverges from the real store, tests
//! written against it can pass while the production behaviour is wrong — the
//! "mock drift" failure mode that P5 recorded five concrete instances of
//! (wrong pagination cursor, `total` recomputed after filtering, a store that
//! ignored `from`/`order_by` entirely, unclamped negative limits, …).
//!
//! Those five were fixed. What cannot be fixed cheaply is the *long tail* of
//! stubs that accept a parameter and discard it: making every one faithful is a
//! large, open-ended task, and doing it opportunistically risks breaking the
//! tests that currently rely on the simplified shape.
//!
//! So the strategy is **explicit, enforced disclosure**: every module that
//! deliberately diverges must say so with a `MOCK DEVIATION` marker, and this
//! test enumerates those modules. A newly introduced deviation therefore fails
//! this gate until it is either implemented faithfully or declared.
//!
//! ## Scope note
//!
//! This is a *disclosure* gate, not a correctness gate: it cannot tell whether a
//! declared deviation is acceptable. It stops deviations from being **silent**.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn mocks_dir() -> PathBuf {
    repo_root().join("synapse-storage/src/test_mocks")
}

/// Files containing the disclosure marker.
fn modules_with_deviation_marker() -> Vec<String> {
    let mut found = Vec::new();
    let entries = fs::read_dir(mocks_dir()).expect("test_mocks dir must be readable");
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.ends_with(".rs") {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            if text.contains("MOCK DEVIATION") || text.contains("SIMPLIFIED STUB") {
                found.push(name.to_string());
            }
        }
    }
    found.sort();
    found
}

/// The set of modules that currently declare a deviation.
///
/// Adding an entry here is a deliberate, reviewable act. If this test fails
/// because a module was added, either implement the deviation faithfully or add
/// it here **together with** a `MOCK DEVIATION` comment explaining the
/// unmodelled dimensions.
const DECLARED_DEVIANTS: &[&str] = &[
    // Device-list windows ignore `from`/`to`/`requester` and the `LIMIT 100`
    // cap: the in-memory store has one global stream counter and no per-device
    // position, so faithful windowing needs a data-model change.
    "device_list.rs",
    // Room blocking is not modelled at all (`block_room` no-op,
    // `get_room_block_status` always `None`).
    "room.rs",
];

#[test]
fn deviation_markers_match_the_declared_list() {
    let found = modules_with_deviation_marker();
    let declared: Vec<String> = DECLARED_DEVIANTS.iter().map(|s| (*s).to_string()).collect();

    assert_eq!(
        found, declared,
        "带 `MOCK DEVIATION` / `SIMPLIFIED STUB` 标记的模块与清单不符。\n\
         新增偏差：请要么把 mock 实现得与真实存储一致，要么在此清单登记**并**\
         就地写 `⚠️ MOCK DEVIATION` 说明哪些维度不可测、为什么。\n\
         移除偏差：请同时更新本清单。\n\
         实际: {found:?}\n清单: {declared:?}"
    );
}

/// The marker must be substantive, not a bare keyword: it has to name what is
/// unmodelled, otherwise it conveys nothing to a future reader.
#[test]
fn deviation_markers_explain_what_is_unmodelled() {
    for name in DECLARED_DEVIANTS {
        let text = fs::read_to_string(mocks_dir().join(name)).expect("module readable");
        let has_explanation = text.contains("unmodelled")
            || text.contains("untestable")
            || text.contains("未建模")
            || text.contains("不可测")
            || text.contains("diverges")
            || text.contains("NOT behaviourally equivalent")
            || text.contains("不建模");
        assert!(
            has_explanation,
            "{name} 的偏差标记必须说明**哪些维度**没有建模/不可测，\
             只写一个 `MOCK DEVIATION` 字样不足以让后续读者判断能否依赖该 mock"
        );
    }
}

/// Returns the body of `fn <name>` in `source`, delimited by that function's
/// own braces.
///
/// Extracting the *body* is the point: a whole-file `contains("block_room")`
/// is satisfied by the unrelated production `pub async fn block_room` and by
/// the assert message, so it never observed the test under guard.
fn function_body<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("fn {name}");
    let start = source.find(&needle)?;
    let after = &source[start..];
    let open = after.find('{')?;
    let mut depth = 0usize;
    for (offset, byte) in after.as_bytes().iter().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&after[open + 1..offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The one test that previously asserted a mock's stub behaviour as though it
/// were coverage must keep saying that it is doing so — by *doing it*, not by
/// containing a keyword somewhere in the file.
///
/// The old form asserted `src.contains("block_room") && src.contains("MOCK
/// DEVIATION")` over the whole of `info.rs`. Both halves are satisfied without
/// the guarded test running: the file declares `pub async fn block_room` at
/// L121, and `MOCK DEVIATION` appears in the assert message. Deleting the
/// `svc.block_room(...)` call therefore left the guard green (sweep B12).
#[test]
fn block_status_test_documents_that_it_tests_the_mock_not_the_service() {
    let src = fs::read_to_string(repo_root().join("synapse-services/src/room/state/info.rs"))
        .expect("info.rs must be readable");
    assert!(
        src.contains("get_room_block_status_is_unmodelled_by_the_in_memory_store"),
        "该测试应明确命名它测的是 mock 的未建模行为；\
         旧名 `get_room_block_status_returns_none_for_unknown_room` 会被误读为\
         服务层对未知房间的行为覆盖"
    );

    let body = function_body(&src, "get_room_block_status_is_unmodelled_by_the_in_memory_store")
        .expect("受守卫的测试函数必须存在且花括号配对");
    assert!(
        body.contains("block_room("),
        "受守卫测试的函数体必须先真的调用 `block_room(`，断言才有意义；\
         仅在文件其它地方出现 `block_room` 字样不算（生产函数名/断言文案都会满足）"
    );
    assert!(
        body.contains("MOCK DEVIATION"),
        "受守卫测试的函数体必须就地记录 `MOCK DEVIATION` 偏差说明，\
         使它测得的是 mock 的未建模行为而不是服务行为"
    );
}

/// Sanity: the mock directory is the one we think it is.
#[test]
fn mock_directory_is_present_and_non_trivial() {
    let entries = fs::read_dir(mocks_dir()).expect("test_mocks dir");
    let count = entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "rs")).count();
    assert!(count >= 20, "test_mocks 目录应有 20+ 个模块，实际 {count}；路径是否变了？");
}
