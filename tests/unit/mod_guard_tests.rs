//! B6-2: 测试文件 mod 守卫。
//!
//! 防止 TST-4 复发（子模块文件存在但父文件未注册 `mod`）。
//! 原理：
//!   1. 扫描 `tests/unit/*.rs` 目录中所有 `.rs` 文件；
//!   2. 从 `tests/unit/mod.rs` 提取已注册的 `mod` 名称；
//!   3. 断言「存在即注册」——每个文件必须在父文件里有 `mod <name>;`。
//!
//! 附加：职责级单源扫描（TST-1/2）—— 测试夹具文件名在多个 mod 注册
//! 视为职责交叉（当前 baseline 为 0，任何新增 >0 即 violation）。

use std::collections::HashSet;
use std::path::Path;

const TESTS_UNIT_DIR: &str = "tests/unit";
const TESTS_UNIT_MOD: &str = "tests/unit/mod.rs";

fn list_unit_rs_files() -> HashSet<String> {
    let dir = Path::new(TESTS_UNIT_DIR);
    // Fail loud: `if let Ok(entries) = read_dir(..)` treated an unreadable /
    // renamed directory as "zero files", and every caller then compared an empty
    // set against the registered mods — the guard passed while scanning nothing
    // (sweep B13).
    let entries = std::fs::read_dir(dir).unwrap_or_else(|error| {
        panic!(
            "mod_guard: cannot read {TESTS_UNIT_DIR} ({error}) — this guard is meaningless without \
             the directory; if the test tree moved, update TESTS_UNIT_DIR in this file"
        )
    });
    let mut names: HashSet<String> = HashSet::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "rs") {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                // 跳过 mod.rs 自身（不是子模块）
                if stem != "mod" {
                    names.insert(stem.to_string());
                }
            }
        }
    }
    assert!(
        names.len() >= 50,
        "mod_guard: {TESTS_UNIT_DIR} yielded only {} .rs modules; the scan looks broken",
        names.len()
    );
    names
}

fn extract_registered_mods() -> HashSet<String> {
    // Same fail-loud rule as `list_unit_rs_files`: an unreadable manifest used to
    // yield an empty registered set, so "every registered mod has a file" passed
    // vacuously.
    let content = std::fs::read_to_string(TESTS_UNIT_MOD)
        .unwrap_or_else(|error| panic!("mod_guard: cannot read {TESTS_UNIT_MOD} ({error})"));
    let mut names: HashSet<String> = HashSet::new();
    // Compiled once: this is a hot loop over every line of `tests/unit/mod.rs`.
    let mod_decl = regex::Regex::new(r"^\s*mod\s+([a-zA-Z0-9_]+)\s*[;{]").expect("static mod pattern");
    // 匹配 `mod <name>;` / `mod <name> {` / `#[cfg(...)]\nmod <name>;`
    for line in content.lines() {
        let trimmed = line.trim();
        // 跳过 #[cfg] 行
        if trimmed.starts_with("#") {
            continue;
        }
        if let Some(cap) = mod_decl.captures(trimmed) {
            names.insert(cap[1].to_string());
        }
    }
    names
}

#[test]
fn test_all_unit_rs_registered_in_mod() {
    let files = list_unit_rs_files();
    let registered = extract_registered_mods();

    let mut unregistered: Vec<String> = files.difference(&registered).cloned().collect();
    unregistered.sort();

    if !unregistered.is_empty() {
        panic!(
            "TST-4 复发：以下测试文件存在但未在 tests/unit/mod.rs 注册：\n{}\n\
             请在 mod.rs 补 `mod <name>;` 或删除未使用的文件。",
            unregistered.join("\n")
        );
    }
}

#[test]
fn test_no_orphan_mod_entries() {
    // TST-4 反向：mod.rs 里注册的 mod 必须有对应文件
    let files = list_unit_rs_files();
    let registered = extract_registered_mods();

    // common、fixtures、snapshots 是目录，不需要 .rs 文件
    let special_entries: HashSet<&str> = ["common", "fixtures", "snapshots"].iter().cloned().collect();
    let mut orphan: Vec<String> =
        registered.difference(&files).filter(|name| !special_entries.contains(name.as_str())).cloned().collect();
    orphan.sort();

    if !orphan.is_empty() {
        panic!(
            "TST-4 反向：以下 mod 在 mod.rs 注册但无对应文件：\n{}\n\
             请删除条目或创建文件。",
            orphan.join("\n")
        );
    }
}

#[test]
fn test_no_duplicate_fixture_names() {
    // TST-1/2 职责级单源：同一 fixture 文件名不应出现在多个测试文件的 mod 名里
    // 此处先收集所有 fixture/ 下的文件名与测试 mod 名的交集
    let fixture_path = format!("{}/fixtures", TESTS_UNIT_DIR);
    let fixture_dir = Path::new(&fixture_path);
    // Fail loud rather than degrading to an empty set: with zero fixture names the
    // intersection below is always empty, so the guard passed no matter what the
    // fixtures were called (sweep B13).
    assert!(
        fixture_dir.is_dir(),
        "mod_guard: {fixture_path} does not exist — this guard exists to compare fixture names \
         against registered mods and cannot do so without the directory"
    );
    let entries = std::fs::read_dir(fixture_dir)
        .unwrap_or_else(|error| panic!("mod_guard: cannot read {fixture_path} ({error})"));
    let fixture_names: HashSet<String> = entries
        .flatten()
        .filter_map(|e| e.path().file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()))
        .collect();
    assert!(
        !fixture_names.is_empty(),
        "mod_guard: {fixture_path} yielded no fixture file names; the comparison would be vacuous"
    );

    let registered = extract_registered_mods();
    let duplicates: Vec<String> = fixture_names.intersection(&registered).cloned().collect();

    if !duplicates.is_empty() {
        panic!(
            "TST-1/2 职责交叉：以下名称同时出现在 tests/unit/fixtures/ 和 mod 注册中：\n{}\n\
             测试文件应使用独立命名（如 `my_feature_fixture_tests`）以区分。",
            duplicates.join("\n")
        );
    }
}
