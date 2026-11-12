use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/installation-contract")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_buddha"))
            .env("BUDDHA_DIR", &self.0)
            .env("PATH", self.0.join("no-git"))
            .env_remove("BUDDHA_SHINZEN_DIR")
            .env_remove("BUDDHA_SHOKU_SHINZEN_DIR")
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn missing_local_corpora_are_unavailable_without_clone() {
    let f = Fixture::new();
    for corpus in [
        "cbeta",
        "tipitaka",
        "gretil",
        "sarit",
        "muktabodha",
        "shinzen",
        "shoku-shinzen",
    ] {
        for (command, arg) in [("search", "--query"), ("fetch", "--id")] {
            let out = f.run(&[&format!("{corpus}-{command}"), arg, "test", "--json"]);
            assert_eq!(
                out.status.code(),
                Some(12),
                "{corpus} {command}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let json: Value = serde_json::from_slice(&out.stderr).unwrap();
            assert_eq!(json["error"]["code"], "DATA_UNAVAILABLE");
            assert!(json["error"]["message"]
                .as_str()
                .unwrap()
                .contains("buddha init"));
        }
    }
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 0);
}

#[test]
fn doctor_reports_warnings_without_changing_status_or_files() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    for dir in [
        "bin",
        "SARIT-corpus",
        "\n  SARIT-corpus",
        "install-backups",
        "cache",
    ] {
        fs::create_dir(f.0.join(dir)).unwrap();
    }
    for file in [
        "bin/buddha",
        "bin/buddha-mcp",
        "daizo-mcp.log",
        "install-backups/old",
    ] {
        fs::write(f.0.join(file), b"abc").unwrap();
    }
    symlink("missing", f.0.join("SHINZEN")).unwrap();
    fs::write(
        f.0.join("cache/cbeta-index.json"),
        r#"[{"path":"/nonexistent-buddha-fixture"}]"#,
    )
    .unwrap();
    let out = f.run(&["--json", "doctor"]);
    assert!(out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["status"], "ok");
    let warnings = value["warnings"].as_array().unwrap();
    for code in [
        "duplicate_name",
        "old_backups",
        "broken_symlink",
        "stale_index_path",
        "old_log",
    ] {
        assert!(warnings.iter().any(|w| w["code"] == code), "{code}");
    }
    assert_eq!(
        warnings
            .iter()
            .find(|w| w["code"] == "old_backups")
            .unwrap()["sizeBytes"],
        3
    );
    assert!(f.0.join("install-backups/old").exists());
}

#[test]
fn uninstall_purge_preserves_shinzen_symlink_targets() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    fs::create_dir(f.0.join("bin")).unwrap();
    for file in ["buddha", "buddha-mcp", "daizo", "daizo-cli", "daizo-mcp"] {
        fs::write(f.0.join("bin").join(file), b"x").unwrap();
    }
    fs::create_dir(f.0.join("GRETIL")).unwrap();
    fs::create_dir(f.0.join("protected")).unwrap();
    symlink(f.0.join("protected"), f.0.join("SHINZEN")).unwrap();
    symlink(f.0.join("protected"), f.0.join("SHOKU_SHINZEN")).unwrap();
    let out = f.run(&["uninstall", "--purge"]);
    assert!(out.status.success());
    assert_eq!(fs::read_dir(f.0.join("bin")).unwrap().count(), 0);
    assert!(!f.0.join("GRETIL").exists());
    assert!(
        f.0.join("SHINZEN").exists()
            && f.0.join("SHOKU_SHINZEN").exists()
            && f.0.join("protected").exists()
    );
}

#[test]
fn help_and_schema_describe_available_commands_and_options() {
    let f = Fixture::new();
    let help = f.run(&["--help"]);
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("SHINZEN") && text.contains("SHOKU_SHINZEN"));
    assert!(!text.contains("Each corpus has search/fetch/pipeline"));
    for (command, flags) in [
        ("sat-fetch", vec!["--url", "--max-chars"]),
        ("sat-detail", vec!["--useid", "--key", "--max-chars"]),
        ("extract-text", vec!["--path"]),
    ] {
        let output = f.run(&["schema", "--command", command]);
        let schema: Value = serde_json::from_slice(&output.stdout).unwrap();
        let args = schema["commands"][0]["args"].as_array().unwrap();
        for flag in flags {
            let arg = args.iter().find(|a| a["flag"] == flag).unwrap();
            assert!(
                !arg["help"].as_str().unwrap_or("").is_empty(),
                "{command} {flag}"
            );
        }
    }
}

#[test]
fn mcp_missing_corpora_return_structured_error_without_clone() {
    use std::io::Write;
    use std::process::Stdio;
    let f = Fixture::new();
    for corpus in ["cbeta", "tipitaka", "gretil", "sarit", "muktabodha"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_buddha"))
            .env("BUDDHA_DIR", &f.0)
            .env("PATH", f.0.join("no-git"))
            .env_remove("BUDDHA_SHINZEN_DIR")
            .env_remove("BUDDHA_SHOKU_SHINZEN_DIR")
            .env("BUDDHA_UNIFIED_TOOLS", "0")
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(), "{}", serde_json::json!({"jsonrpc":"2.0", "id":1, "method":"tools/call", "params":{"name":format!("{corpus}_search"), "arguments":{"query":"test"}}})).unwrap();
        let out = child.wait_with_output().unwrap();
        let value: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            value["result"]["_meta"]["error"]["code"], "corpus_not_installed",
            "{corpus}: {value}"
        );
        assert!(value["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("buddha init"));
    }
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 0);
}

#[test]
fn installer_sets_executable_mode_before_atomic_move() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let script =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/install.sh"))
            .unwrap();
    let start = script.find("cp -f \"$CLI_SRC\" \"$CLI_TMP\"").unwrap();
    let end = script[start..].find("mv -f \"$CLI_TMP\"").unwrap() + start;
    assert!(script[start..end].contains("chmod 755 \"$CLI_TMP\""));
    let source = f.0.join("source");
    fs::write(&source, b"fixture").unwrap();
    let status = Command::new("bash").arg("-c").arg("CLI_TMP=$(mktemp \"$BIN_OUT/.buddha.XXXXXXXX\"); cp -f \"$CLI_SRC\" \"$CLI_TMP\"; chmod 755 \"$CLI_TMP\"; mv -f \"$CLI_TMP\" \"$BIN_OUT/buddha\"").env("BIN_OUT", &f.0).env("CLI_SRC", &source).status().unwrap();
    assert!(status.success());
    assert_eq!(
        fs::metadata(f.0.join("buddha"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
}
