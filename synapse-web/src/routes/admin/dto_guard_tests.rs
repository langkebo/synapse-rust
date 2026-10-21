//! `routes/admin/**` 下每个 `Deserialize` struct 都必须**显式决定**未知字段怎么处理。
//!
//! N-1（`docs/audit/BACKEND_CONTRACT_SURFACE_VERIFICATION_2026-10-19.md`）的原话是
//! 「有 struct ≠ 有校验」：`#[derive(Deserialize)]` 默认静默丢弃未知字段。真正难发现的不是
//! "没加 deny"，而是**同一请求里外壳 deny、内核不 deny**——严格的外层承诺把"未知字段会被拒绝"
//! 变成了假象，而错误就藏在被静默丢掉的那一层里。
//!
//! 这里的断言不看注释、不靠约定，只认两件事实：
//!   1. 这个 struct 有没有 `#[serde(deny_unknown_fields)]`；
//!   2. 它有没有登记在 `test-support/admin_dto_open_fields.txt`。
//!
//! 二者必居其一：既没 deny 又没登记 ⇒ 红（"没人决定过"）；登记了却 deny ⇒ 红（登记过期）。
//! 与 `admin_rbac_tests` 的登记表同一套双向语义。

use std::collections::BTreeSet;

/// 一个 struct 的扫描结果。
struct DeserializeStruct {
    name: String,
    file: String,
    line: usize,
    denies_unknown: bool,
}

fn admin_rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            admin_rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// 收集 `pub struct` 及其上方**连续的**属性/文档块。
fn scan_deserialize_structs() -> Vec<DeserializeStruct> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes/admin");
    let mut files = Vec::new();
    admin_rust_files(&root, &mut files);
    files.sort();

    let mut found = Vec::new();
    for path in files {
        let source = std::fs::read_to_string(&path).expect("admin source must be readable");
        let lines: Vec<&str> = source.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("pub struct ") else {
                continue;
            };
            let name: String = rest.chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').collect();
            if name.is_empty() {
                continue;
            }

            // 往上走：跳过文档注释与其它属性，遇到第一个非属性行停。
            let mut attributes = Vec::new();
            let mut cursor = index;
            while cursor > 0 {
                cursor -= 1;
                let above = lines[cursor].trim_start();
                if above.starts_with("///") || above.starts_with("#[") {
                    attributes.push(above.to_string());
                    continue;
                }
                break;
            }

            if !attributes.iter().any(|attribute| attribute.contains("Deserialize")) {
                continue;
            }

            // 只看**真属性**行。文档注释里经常出现 `deny_unknown_fields` 这个词
            // （"Deliberately **not** `deny_unknown_fields`" 这种说明），按字符串
            // 全文匹配会把"明确说了不加"的 struct 判成"已经加了"——本守卫第一版
            // 就踩了这个坑，4 条登记被判成过期。
            let real_attributes: Vec<&String> =
                attributes.iter().filter(|attribute| attribute.starts_with("#[")).collect();

            found.push(DeserializeStruct {
                name,
                file: path
                    .strip_prefix(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
                line: index + 1,
                denies_unknown: real_attributes.iter().any(|attribute| attribute.contains("deny_unknown_fields")),
            });
        }
    }
    found
}

fn declared_open_fields() -> BTreeSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../test-support/admin_dto_open_fields.txt");
    std::fs::read_to_string(&path)
        .expect("the admin DTO register must be readable")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// 每个 admin 请求 DTO 要么 deny，要么登记为"有意开放"。
#[test]
fn every_admin_dto_decides_what_to_do_with_unknown_fields() {
    let structs = scan_deserialize_structs();
    let declared = declared_open_fields();

    // 守卫的守卫：扫描器失效（路径写错 / 正则不匹配）时两边都会是空的。
    assert!(
        structs.len() > 20,
        "scanned only {} Deserialize structs under routes/admin — the scanner is probably broken, \
         so this test would be asserting nothing",
        structs.len()
    );

    let undecided: Vec<String> = structs
        .iter()
        .filter(|item| !item.denies_unknown && !declared.contains(&item.name))
        .map(|item| format!("{}:{} {} — no `deny_unknown_fields`, not registered", item.file, item.line, item.name))
        .collect();

    let mut problems: Vec<String> = Vec::new();
    if !undecided.is_empty() {
        problems.push(format!(
            "admin 面这些请求 DTO 既没 deny 也没登记（= 没人决定过未知字段怎么处理）：\n{}\n\
             Body 面：与外壳一致，加 `#[serde(deny_unknown_fields)]`。\n\
             Query 面：保持开放，但必须登记到 test-support/admin_dto_open_fields.txt。",
            undecided.join("\n")
        ));
    }

    // 反向：登记了却已经 deny、或 struct 已删 ⇒ 登记过期。
    let by_name: BTreeSet<&str> = structs.iter().map(|item| item.name.as_str()).collect();
    let denying: BTreeSet<&str> =
        structs.iter().filter(|item| item.denies_unknown).map(|item| item.name.as_str()).collect();
    let stale: Vec<String> = declared
        .iter()
        .filter(|name| !by_name.contains(name.as_str()) || denying.contains(name.as_str()))
        .map(|name| {
            if denying.contains(name.as_str()) {
                format!("{name} — 已经 deny 了，把这行从登记表删掉")
            } else {
                format!("{name} — routes/admin 下已经没有这个 struct")
            }
        })
        .collect();
    if !stale.is_empty() {
        problems.push(format!("登记表里有过期条目：\n{}", stale.join("\n")));
    }

    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
}
