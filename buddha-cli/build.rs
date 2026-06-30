use std::process::Command;

fn main() {
    // Ensure rebuild when package metadata changes (e.g., version bump).
    println!("cargo:rerun-if-changed=Cargo.toml");

    // ビルド日時を設定（date コマンドが無い環境でもビルドを止めない）
    let build_date = Command::new("date")
        .args(["+%Y-%m-%d %H:%M:%S"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_DATE={}", build_date);

    // Mirror package version into a build-script driven env var so version bumps
    // reliably propagate even if cargo doesn't rebuild on manifest-only changes.
    if let Ok(ver) = std::env::var("CARGO_PKG_VERSION") {
        println!("cargo:rustc-env=BUDDHA_VERSION={}", ver);
    }

    // Git コミットハッシュを取得（利用可能な場合）
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
    {
        if output.status.success() {
            let git_hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("cargo:rustc-env=GIT_HASH={}", git_hash);
        }
    }

    // build.rs 自体が変更されたときのみ再実行する（Cargo.toml は上で指定済み）。
    // 注: BUILD_DATE はこのスクリプトが再実行されたときだけ更新される。
    println!("cargo:rerun-if-changed=build.rs");
}
