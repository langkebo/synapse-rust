//! 守卫：生产源码里不得再残留 "Reserved"/"constructor parity" 死字段标记。
//!
//! 背景（AGENTS.md 反冗余铁律 1 + 8）：一批 `Reserved for future use` /
//! `constructor parity` 字段删除后，如果只删字段不删说明注释，后来者很容易照猫画虎
//! 再塞回同名死字段。这个守卫把标记本身变成门禁：`synapse-services/src`、
//! `synapse-web/src`、`synapse-storage/src` 下任何 `.rs` 文件出现标记即失败，
//! 并把 `path:line` 列表打进断言消息。
//!
//! 正则里的 `\b` 是必要的：`(?i)Reserved:` 会误伤无关的 `preserved:`（例如
//! `synapse-storage/src/sliding_sync/tests.rs` 的断言消息 `expected base query preserved:`）。
//! `marker_regex_is_well_formed` 用正/负对照锁住这一点 —— 没有它，守卫既可能假绿
//! （正则写错）也可能假红（误伤正常文本）。
//!
//! **红证明**：在 `synapse-services/src` 任一生产结构体里临时加一行
//! `// Reserved; constructor parity`，`dead_field_markers_absent_from_production_sources`
//! 必须失败并列出该 `path:line`；删掉探针后转绿。

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

/// 死字段标记正则。
///
/// `\b` 保证 `Reserved`/`reserved` 必须是一个完整单词的起点，从而排除 `preserved:`。
const MARKER_PATTERN: &str = r"(?i)\b(Reserved;|Reserved for future use|Reserved:|stored for constructor parity)";

/// 需要扫描的生产源码根目录（相对仓库根）。
const SCAN_ROOTS: [&str; 3] = ["synapse-services/src", "synapse-web/src", "synapse-storage/src"];

/// 仓库根：由 `CARGO_MANIFEST_DIR` 推导，因此与运行时 CWD 无关。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn marker_regex() -> Regex {
    Regex::new(MARKER_PATTERN).expect("MARKER_PATTERN 必须能编译")
}

/// 递归收集 `*.rs` 文件（不依赖 walkdir）。
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// 正/负对照：正则必须命中真实标记，且不得命中无关的 `preserved:`。
///
/// 这是"门禁自证"的第一层：如果 `MARKER_PATTERN` 被改坏成永不命中，本测试变红；
/// 如果改坏成误伤 `preserved:`，本测试同样变红。
#[test]
fn marker_regex_is_well_formed() {
    let re = marker_regex();
    for probe in [
        "// Reserved; constructor parity",
        "// Reserved for future use by room state hooks; stored for constructor parity.",
        "// Reserved: stored for potential direct use by RoomService methods;",
        "stored for constructor parity",
    ] {
        assert!(re.is_match(probe), "正对照失败：正则必须命中 {probe:?}");
    }
    assert!(
        !re.is_match(
            r#"assert!(sql.starts_with("SELECT * FROM t WHERE 1=1"), "expected base query preserved: {sql}");"#
        ),
        "负对照失败：`preserved:` 不是死字段标记，正则会误伤它"
    );
}

/// 主门禁：三个生产源码目录下零命中。
#[test]
fn dead_field_markers_absent_from_production_sources() {
    let root = repo_root();
    let re = marker_regex();

    let mut files = Vec::new();
    for rel in SCAN_ROOTS {
        collect_rs_files(&root.join(rel), &mut files);
    }
    assert!(!files.is_empty(), "守卫必须至少扫描到一个 .rs 文件（仓库根推导错了？root={})", root.display());

    let mut offenders = Vec::new();
    for path in &files {
        let content = fs::read_to_string(path).expect("生产源码必须可按 UTF-8 读取");
        for (idx, line) in content.lines().enumerate() {
            if re.is_match(line) {
                let rel = path.strip_prefix(&root).unwrap_or(path);
                offenders.push(format!("{}:{}", rel.display(), idx + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "生产源码中不得再出现 Reserved/constructor-parity 死字段标记；命中 {} 处：\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}
