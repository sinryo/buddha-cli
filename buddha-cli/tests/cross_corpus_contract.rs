use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Corpus(PathBuf);
impl Corpus {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/relay-work")
            .join(format!(
                "fixture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&root).unwrap();
        let xml="<TEI>\n<teiHeader><fileDesc><titleStmt><title>Fixture</title></titleStmt></fileDesc></teiHeader>\n<text><body><p>\n<lb n=\"0749c22\"/>應無所住\n<lb n=\"0749c23\"/>而生其心。六大無礙\n</p></body></text></TEI>";
        for (path,body) in [("xml-p5/T/T08/T08n0235.xml",xml),("SHINZEN/kunten_tei_xml/volume_13_page_006.xml","<TEI><text><body><lb n=\"12\"/>六大無礙<lb n=\"13\"/>應無所住而生其心</body></text></TEI>")] {
   let p=root.join(path);fs::create_dir_all(p.parent().unwrap()).unwrap();fs::write(p,body).unwrap();
  }
        Self(root)
    }
    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(
            std::env::var("BUDDHA_CONTRACT_BIN")
                .unwrap_or_else(|_| env!("CARGO_BIN_EXE_buddha").into()),
        )
        .env("BUDDHA_DIR", &self.0)
        .args(args)
        .output()
        .unwrap()
    }
    fn json(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
}
impl Drop for Corpus {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn all_search_groups_hits_skips_absent_corpora_and_bounds_total() {
    let c = Corpus::new();
    let v = c.json(&["search", "六大無礙", "--json"]);
    let m = &v["result"]["_meta"];
    assert_eq!(m["corpora"].as_array().unwrap().len(), 2);
    assert_eq!(m["skipped"][0]["source"], "shoku_shinzen");
    for g in m["corpora"].as_array().unwrap() {
        assert_eq!(g["totalFilesMatched"], 1);
        assert_eq!(g["results"].as_array().unwrap().len(), 1);
        assert!(g["results"][0]["nextAction"]["command"]
            .as_str()
            .unwrap()
            .contains("-fetch"));
    }
    let all = c.json(&[
        "search",
        "六大無礙",
        "--source",
        "all",
        "--max-results",
        "1",
        "--json",
    ]);
    assert_eq!(
        all["result"]["_meta"]["skipped"].as_array().unwrap().len(),
        5
    );
    assert_eq!(
        all["result"]["_meta"]["corpora"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["results"].as_array().unwrap().len())
            .sum::<usize>(),
        1
    );
}
#[test]
fn verify_four_statuses_and_location_mismatch() {
    let c = Corpus::new();
    for (quote, status) in [
        ("應無所住而生其心", "exact"),
        ("応無所住而生其心", "variant"),
        ("應無所住而生妙心", "partial"),
        ("完全不存在的文字", "not_found"),
    ] {
        let v = c.json(&["verify", "--id", "T08n0235", "--quote", quote, "--json"]);
        let m = &v["result"]["_meta"];
        assert_eq!(m["status"], status, "{v}");
        if status == "variant" {
            assert_eq!(m["differences"][0], json!({"quote":"応","text":"應"}));
        }
        if status == "exact" {
            assert_eq!(m["location"]["lbStart"], "0749c22");
            assert_eq!(m["location"]["lbEnd"], "0749c23");
            assert!(m["citation"].as_str().unwrap().contains("235"));
        }
        if status == "partial" {
            assert_eq!(m["matchedQuote"], "應無所住而生");
            assert!(m["contextAfter"].as_str().unwrap().contains("心"));
        }
        if status == "not_found" {
            assert_eq!(m["nextActions"].as_array().unwrap().len(), 2);
        }
    }
    let v = c.json(&[
        "verify",
        "--id",
        "T08n0235",
        "--quote",
        "應無所住而生其心",
        "--lb",
        "0750a01",
        "--json",
    ]);
    assert_eq!(v["result"]["_meta"]["locationMismatch"], true);
}
#[test]
fn batch_fetch_keeps_successes_and_per_id_locators() {
    let c = Corpus::new();
    let v = c.json(&[
        "fetch",
        "T08n0235@0749c22",
        "SZ13p0006:12",
        "T9999",
        "--json",
    ]);
    let results = v["result"]["_meta"]["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0]["status"], "ok");
    assert!(results[0]["text"].as_str().unwrap().contains("應無所住"));
    assert_eq!(results[1]["status"], "ok");
    assert_eq!(results[2]["status"], "error");
    let bounded = c.json(&[
        "fetch",
        "T08n0235",
        "SZ13p0006",
        "--max-chars",
        "3",
        "--json",
    ]);
    for r in bounded["result"]["_meta"]["results"].as_array().unwrap() {
        assert_eq!(r["text"].as_str().unwrap().chars().count(), 3);
        assert_eq!(r["truncated"], true);
    }
}
#[test]
fn cli_action_objects_are_commands_and_missing_ids_have_recovery() {
    let c = Corpus::new();
    let v = c.json(&[
        "cbeta-search",
        "--query",
        "六大無礙",
        "--max-results",
        "1",
        "--json",
    ]);
    fn no_tools(v: &Value) {
        match v {
            Value::Object(o) => {
                assert!(!o.contains_key("tool"), "{v}");
                for x in o.values() {
                    no_tools(x);
                }
            }
            Value::Array(a) => {
                for x in a {
                    no_tools(x);
                }
            }
            _ => {}
        }
    }
    no_tools(&v);
    let missing = c.run(&["fetch", "T9999", "--json"]);
    assert_eq!(missing.status.code(), Some(10));
    let e: String = String::from_utf8(missing.stderr).unwrap();
    assert!(e.contains("buddha resolve"));
    assert!(e.contains("buddha cbeta-title-search"));
    let p = c.0.join("xml-p5/T/T09/T09n0924A.xml");
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, "<TEI><text>text</text></TEI>").unwrap();
    let missing = c.run(&["fetch", "T18n0924", "--json"]);
    let e = String::from_utf8(missing.stderr).unwrap();
    assert!(e.contains("T09n0924A"), "{e}");
}
#[test]
fn missing_direct_ids_never_advise_fetch_and_zero_results_are_short() {
    let c = Corpus::new();
    for id in ["DN99999", "SZ99p9999", "SKZ99p9999"] {
        let v = c.json(&["resolve", "--query", id, "--json"]);
        for candidate in v["result"]["_meta"]["candidates"].as_array().unwrap() {
            assert!(candidate.get("fetch").is_none(), "{v}");
        }
    }
    let v = c.run(&["--text", "cbeta-search", "不存在"]);
    let text = String::from_utf8(v.stdout).unwrap();
    assert!(text.starts_with("0 matching files\n"), "{text}");
    assert!(!text.contains("offset 0"));
}

#[test]
fn mcp_stdio_exposes_and_runs_shared_operations() {
    use std::io::Write;
    use std::process::Stdio;
    let c = Corpus::new();
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search","arguments":{"query":"六大無礙"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"verify","arguments":{"id":"T08n0235","quote":"応無所住而生其心"}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"fetch","arguments":{"ids":["T08n0235@0749c22","SZ13p0006:12","T9999"]}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"fetch","arguments":{"ids":vec!["T08n0235";11]}}}),
        json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"verify","arguments":{"id":"2,7,98,38,0","source":"sat","quote":"六大無礙"}}}),
    ];
    let mut child = Command::new(env!("CARGO_BIN_EXE_buddha"))
        .env("BUDDHA_DIR", &c.0)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        for req in requests {
            writeln!(input, "{req}").unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let responses = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .collect::<Vec<_>>();
    let get = |id| responses.iter().find(|v| v["id"] == id).unwrap();
    assert!(get(1)["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("verify"));
    let tools = get(2)["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|v| v["name"] == "verify"));
    let fetch = tools.iter().find(|v| v["name"] == "fetch").unwrap();
    assert_eq!(fetch["inputSchema"]["properties"]["ids"]["maxItems"], 10);
    let search = tools.iter().find(|v| v["name"] == "search").unwrap();
    assert_eq!(search["inputSchema"]["required"], json!(["query"]));
    let m = &get(3)["result"]["structuredContent"];
    assert_eq!(m["corpora"].as_array().unwrap().len(), 2);
    assert_eq!(m["corpora"][0]["results"][0]["nextAction"]["tool"], "fetch");
    assert_eq!(get(4)["result"]["structuredContent"]["status"], "variant");
    assert_eq!(
        get(5)["result"]["structuredContent"]["results"][2]["status"],
        "error"
    );
    assert_eq!(get(6)["result"]["isError"], true);
    assert_eq!(get(7)["result"]["isError"], true);
    assert!(get(7)["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("未対応"));
    for command in ["search", "fetch", "verify"] {
        let v = c.json(&["schema", "--command", command]);
        assert_eq!(v["commands"][0]["name"], command);
    }
}

#[test]
fn verify_uses_corrections_ignores_reading_aids_and_honours_repeated_locations() {
    let c = Corpus::new();
    fs::write(c.0.join("SHINZEN/kunten_tei_xml/volume_13_page_006.xml"),"<TEI><text><body><lb n=\"12\"/>六大<add>ハ</add>無<metamark>二</metamark>礙</body></text></TEI>").unwrap();
    let v = c.json(&[
        "verify",
        "--id",
        "SZ13p0006",
        "--quote",
        "六大無礙",
        "--json",
    ]);
    assert_eq!(v["result"]["_meta"]["status"], "exact", "{v}");
    assert_eq!(v["result"]["_meta"]["location"]["lineNumber"], 12);
    assert!(v["result"]["_meta"]["text"]
        .as_str()
        .unwrap()
        .contains('ハ'));
    let path =
        c.0.join("SHOKU_SHINZEN/kunten_tei_xml/volume_02_page_001.xml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        "<TEI><text><body><lb n=\"3\"/>六大無誤</body></text></TEI>",
    )
    .unwrap();
    let path = c.0.join("SHOKU_SHINZEN/corrections/corrections.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        r#"{"entries":[{"page":"SKZ02p0001","line":3,"old":"誤","new":"礙"}]}"#,
    )
    .unwrap();
    let v = c.json(&[
        "verify",
        "--id",
        "SKZ02p0001",
        "--quote",
        "六大無礙",
        "--json",
    ]);
    assert_eq!(v["result"]["_meta"]["status"], "exact", "{v}");
    assert_eq!(v["result"]["_meta"]["location"]["lineNumber"], 3);
    fs::write(c.0.join("xml-p5/T/T08/T08n0235.xml"),"<TEI><text><body>\n<lb n=\"0001a01\"/>應無所住而生其心\n<lb n=\"0002a01\"/>応無所住而生其心\n</body></text></TEI>").unwrap();
    let v = c.json(&[
        "verify",
        "--id",
        "T08n0235",
        "--quote",
        "応無所住而生其心",
        "--lb",
        "0002a01",
        "--json",
    ]);
    assert_eq!(v["result"]["_meta"]["status"], "exact", "{v}");
    assert!(
        v["result"]["_meta"].get("locationMismatch").is_none(),
        "{v}"
    );
}

#[test]
fn all_local_json_actions_are_executable_cli_commands() {
    let c = Corpus::new();
    for path in [
        "tipitaka-xml/romn/fixture.xml",
        "GRETIL/1_sanskr/tei/fixture.xml",
        "SARIT-corpus/fixture.xml",
        "MUKTABODHA/fixture.xml",
        "SHOKU_SHINZEN/kunten_tei_xml/volume_02_page_001.xml",
    ] {
        let path = c.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path,"<TEI><teiHeader><fileDesc><titleStmt><title>Fixture</title></titleStmt></fileDesc></teiHeader><text><body><lb n=\"1\"/>六大無礙 dharmaḥ</body></text></TEI>").unwrap();
    }
    fn check(v: &Value) {
        match v {
            Value::Object(o) => {
                assert!(!o.contains_key("tool"), "{v}");
                if let Some(command) = o.get("command") {
                    assert!(command.as_str().unwrap().starts_with("buddha "));
                }
                for value in o.values() {
                    check(value);
                }
            }
            Value::Array(a) => {
                for v in a {
                    check(v);
                }
            }
            _ => {}
        }
    }
    for (source, id) in [
        ("cbeta", "T08n0235"),
        ("tipitaka", "fixture"),
        ("gretil", "fixture"),
        ("sarit", "fixture"),
        ("muktabodha", "fixture"),
        ("shinzen", "SZ13p0006"),
        ("shoku-shinzen", "SKZ02p0001"),
    ] {
        for command in [format!("{source}-search"), format!("{source}-fetch")] {
            let args = if command.ends_with("-fetch") {
                vec![command.as_str(), "--id", id, "--json"]
            } else {
                vec![
                    command.as_str(),
                    "--query",
                    "六大無礙",
                    "--max-results",
                    "1",
                    "--json",
                ]
            };
            check(&c.json(&args));
        }
    }
    for source in ["shinzen", "shoku-shinzen"] {
        check(&c.json(&[&format!("{source}-work-catalog"), "--json"]));
    }
    check(&c.json(&["resolve", "--query", "Fixture", "--json"]));
}

#[test]
fn latin_selection_batch_default_and_locator_continuation() {
    let c = Corpus::new();
    for path in [
        "tipitaka-xml/romn/fixture.xml",
        "GRETIL/1_sanskr/tei/fixture.xml",
    ] {
        let path = c.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            "<TEI><text><body>dharmaḥ saṃghātaṃ</body></text></TEI>",
        )
        .unwrap();
    }
    let v = c.json(&["search", "dharmaḥ", "--source", "local", "--json"]);
    let m = &v["result"]["_meta"];
    assert_eq!(m["corpora"].as_array().unwrap().len(), 2);
    assert_eq!(m["skipped"].as_array().unwrap().len(), 2);
    for g in m["corpora"].as_array().unwrap() {
        assert_eq!(g["totalFilesMatched"], 1);
    }
    for (quote, status) in [("saṃgha\u{304}taṃ", "exact"), ("saṁghātaṁ", "variant")] {
        let v = c.json(&[
            "verify", "--id", "fixture", "--source", "tipitaka", "--quote", quote, "--json",
        ]);
        assert_eq!(v["result"]["_meta"]["status"], status, "{v}");
    }
    fs::write(
        c.0.join("xml-p5/T/T08/T08n0235.xml"),
        format!(
            "<TEI><text><body><lb n=\"0749c22\"/>{}</body></text></TEI>",
            "無".repeat(2200)
        ),
    )
    .unwrap();
    let v = c.json(&["fetch", "T08n0235@0749c22", "SZ13p0006", "--json"]);
    let first = &v["result"]["_meta"]["results"][0];
    assert_eq!(first["text"].as_str().unwrap().chars().count(), 2000);
    assert!(first["nextAction"]["command"]
        .as_str()
        .unwrap()
        .contains("--lb 0749c22"));
    assert!(first["nextAction"]["command"]
        .as_str()
        .unwrap()
        .contains("--start-char 2000"));
}
