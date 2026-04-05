use std::fs;
use std::path::{Path, PathBuf};

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, out);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
}

#[test]
fn test_no_placeholder_auth_user_ignores_in_handlers() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let handlers_dir = root.join("src").join("web").join("routes").join("handlers");
    let mut files = Vec::new();
    collect_rs_files(&handlers_dir, &mut files);
    files.sort();

    let mut violations = Vec::new();
    for file in files {
        let content = match fs::read_to_string(&file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if content.contains("let _ = auth_user;") {
            violations.push(file.display().to_string());
        }
    }

    assert!(
        violations.is_empty(),
        "Found placeholder-style auth ignores in handlers:\n{}",
        violations.join("\n")
    );
}
