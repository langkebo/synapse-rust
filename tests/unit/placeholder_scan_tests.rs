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

fn load_allowlist_entries(file: &Path) -> Vec<String> {
    let content = fs::read_to_string(file).expect("allowlist should be readable");

    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect()
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

#[test]
fn test_empty_json_successes_are_allowlisted() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let routes_dir = root.join("src").join("web").join("routes");
    let allowlist_file = root.join("scripts").join("shell_routes_allowlist.txt");
    let allowlist = load_allowlist_entries(&allowlist_file);
    let mut files = Vec::new();
    collect_rs_files(&routes_dir, &mut files);
    files.sort();

    let mut violations = Vec::new();
    for file in files {
        let content = match fs::read_to_string(&file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (index, line) in content.lines().enumerate() {
            if !line.contains("Ok(empty_json())") {
                continue;
            }

            let relative = file
                .strip_prefix(&routes_dir)
                .expect("file should live under routes_dir")
                .to_string_lossy()
                .replace('\\', "/");
            let entry = format!("{}:{}", relative, index + 1);
            if !allowlist.iter().any(|item| item == &entry) {
                violations.push(entry);
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Found Ok(empty_json()) matches missing from shell route allowlist:\n{}",
        violations.join("\n")
    );
}
