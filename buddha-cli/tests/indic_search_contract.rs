use serde_json::Value;
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn indic_search_normalization_loose_and_retry_contract() {
    let root = std::env::temp_dir().join(format!("buddha-indic-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    for path in [
        "GRETIL/1_sanskr/tei/fixture.xml",
        "SARIT-corpus/fixture.xml",
        "MUKTABODHA/fixture.xml",
        "tipitaka-xml/romn/fixture.xml",
    ] {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            "<TEI><text><body><p>saṃghātaṃ dharmaḥ</p><p>saṁgha\u{304}taṁ</p></body></text></TEI>",
        )
        .unwrap();
    }
    for corpus in ["tipitaka", "gretil", "sarit", "muktabodha"] {
        for (query, loose, count) in [
            ("saṁghātaṁ", false, 1),
            ("saṃgha\u{304}taṃ", false, 1),
            ("samghatam", false, 0),
            ("samghatam", true, 1),
            ("saMghAtaM", true, 1),
        ] {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_buddha"));
            cmd.env("BUDDHA_DIR", &root).args([
                "--json",
                &format!("{corpus}-search"),
                "--query",
                query,
            ]);
            if loose {
                cmd.arg("--loose");
            }
            let out = cmd.output().unwrap();
            assert!(
                out.status.success(),
                "{corpus}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let v: Value = serde_json::from_slice(&out.stdout).unwrap();
            let meta = &v["result"]["_meta"];
            assert_eq!(meta["totalFiles"], count, "{corpus}/{query}");
            assert_eq!(meta["query"], query);
            assert!(v["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains(query));
            assert!(meta["searchPattern"].as_str().unwrap().chars().count() <= 200);
            assert_eq!(meta["queryMode"], if loose { "loose" } else { "phrase" });
            if count == 0 {
                assert!(meta["hint"].as_str().unwrap().contains("--loose"));
            }
            if loose {
                assert!(meta["results"][0]["matches"]
                    .to_string()
                    .contains("saṃghātaṃ"));
            }
        }
        let out = Command::new(env!("CARGO_BIN_EXE_buddha"))
            .env("BUDDHA_DIR", &root)
            .args([
                "--json",
                &format!("{corpus}-search"),
                "--query",
                "dharma.*",
                "--loose",
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&out.stderr).contains("INVALID_ARGUMENT"));
    }

    // Exercise the public stdio boundary: unified/legacy dispatch, metadata,
    // preserved source highlights, retry actions, validation and tool schemas.
    for legacy in [false, true] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_buddha"))
            .env("BUDDHA_DIR", &root)
            .env("BUDDHA_UNIFIED_TOOLS", if legacy { "0" } else { "1" })
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut requests = vec![serde_json::json!({"jsonrpc":"2.0","id":0,"method":"tools/list"})];
        for corpus in ["tipitaka", "gretil", "sarit", "muktabodha"] {
            for (query, loose) in [
                ("samghatam", true),
                ("saṁghātaṁ", false),
                ("samghatam", false),
                ("dharma.*", true),
                ("^dharma", true),
                ("dharma|sa", true),
            ] {
                requests.push(serde_json::json!({"jsonrpc":"2.0","id":requests.len(),"method":"tools/call","params":{"name":if legacy {format!("{corpus}_search")} else {"search".to_owned()},"arguments":{"source":corpus,"query":query,"loose":loose,"maxResults":1}}}));
            }
        }
        for corpus in ["gretil", "sarit", "muktabodha"] {
            requests.push(serde_json::json!({"jsonrpc":"2.0","id":requests.len(),"method":"tools/call","params":{"name":if legacy {format!("{corpus}_pipeline")} else {"pipeline".to_owned()},"arguments":{"source":corpus,"query":"samghatam","loose":true,"autoFetch":true,"highlight":"samghatam","maxResults":1}}}));
        }
        {
            let mut input = child.stdin.take().unwrap();
            for req in &requests {
                writeln!(input, "{req}").unwrap();
            }
        }
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let replies: Vec<Value> = String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(replies.len(), requests.len());
        let tools = replies[0]["result"]["tools"].as_array().unwrap();
        for name in if legacy {
            vec![
                "tipitaka_search",
                "gretil_search",
                "sarit_search",
                "muktabodha_search",
                "gretil_pipeline",
                "sarit_pipeline",
                "muktabodha_pipeline",
            ]
        } else {
            vec!["search", "pipeline"]
        } {
            let tool = tools.iter().find(|t| t["name"] == name).unwrap();
            assert_eq!(
                tool["inputSchema"]["properties"]["loose"]["type"],
                "boolean"
            );
        }
        for (req, reply) in requests.iter().zip(&replies).skip(1) {
            let args = &req["params"]["arguments"];
            let result = &reply["result"];
            if args["query"].as_str().unwrap().contains(['.', '^', '|']) {
                assert_eq!(result["isError"], true);
                assert_eq!(result["_meta"]["error"]["code"], "INVALID_ARGUMENT");
            } else if args["query"] == "samghatam" && args["loose"] == false {
                assert_eq!(result["_meta"]["totalFiles"], 0);
                assert_eq!(result["_meta"]["nextActions"][0]["args"]["loose"], true);
                assert_eq!(result["_meta"]["nextActions"][0]["args"]["maxResults"], 1);
                if !legacy {
                    assert_eq!(
                        result["structuredContent"]["nextActions"][0]["args"]["loose"],
                        true
                    );
                }
            } else {
                assert_eq!(result["_meta"]["totalFiles"], 1, "{req}: {reply}");
                assert_eq!(
                    result["_meta"]["queryMode"],
                    if args["loose"] == true {
                        "loose"
                    } else {
                        "phrase"
                    }
                );
                assert!(result["_meta"]["results"][0]["matches"]
                    .to_string()
                    .contains("saṃghātaṃ"));
                if !legacy {
                    assert_eq!(
                        result["structuredContent"]["queryMode"],
                        result["_meta"]["queryMode"]
                    );
                }
                if req["params"]["name"].as_str().unwrap().contains("pipeline") {
                    assert!(result["content"].to_string().contains("saṃghātaṃ"));
                }
            }
        }
    }
    let out = Command::new(env!("CARGO_BIN_EXE_buddha"))
        .env("BUDDHA_DIR", &root)
        .args(["--text", "tipitaka-search", "samghatam"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("--loose"));
    let out = Command::new(env!("CARGO_BIN_EXE_buddha"))
        .env("BUDDHA_DIR", &root)
        .args([
            "--json",
            "gretil-pipeline",
            "--query",
            "samghatam",
            "--loose",
            "--autofetch",
            "--highlight",
            "samghatam",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["result"]["_meta"]["queryMode"], "loose");
    assert_eq!(v["result"]["_meta"]["totalFiles"], 1);
    assert!(v["result"]["content"]
        .to_string()
        .contains(">>> saṃghātaṃ <<<"));
    for corpus in ["tipitaka", "gretil", "sarit", "muktabodha"] {
        let out = Command::new(env!("CARGO_BIN_EXE_buddha"))
            .args(["schema", "--command", &format!("{corpus}-search")])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("loose"));
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn resolve_missing_cbeta_id_never_suggests_fetch() {
    let root = std::env::temp_dir().join(format!("buddha-resolve-missing-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buddha"))
        .env("BUDDHA_DIR", &root)
        .args(["resolve", "--query", "T2211", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let candidate = &v["result"]["_meta"]["candidates"][0];
    assert!(candidate.get("fetch").is_none());
    assert_eq!(candidate["resolvedBy"], "unavailable-id");
    assert!(candidate["note"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("sat"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_source_searches_available_local_corpora() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target/relay-work/empty-search");
    fs::create_dir_all(&root).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buddha"))
        .env("BUDDHA_DIR", &root)
        .args(["search", "六大無礙", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        result["result"]["_meta"]["skipped"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    fs::remove_dir_all(root).unwrap();
}
