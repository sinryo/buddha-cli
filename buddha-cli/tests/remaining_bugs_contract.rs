use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};
struct Corpus(PathBuf);
impl Corpus {
    fn new(name: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target")
            .join(format!("regression-{name}-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn put(&self, path: &str, text: &str) {
        let p = self.0.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_buddha"))
            .env("BUDDHA_DIR", &self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn mcp(&self, calls: Vec<(&str, Value)>) -> Vec<Value> {
        let mut c = Command::new(env!("CARGO_BIN_EXE_buddha"))
            .env("BUDDHA_DIR", &self.0)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        {
            let mut stdin = c.stdin.take().unwrap();
            for (i, (name, args)) in calls.into_iter().enumerate() {
                writeln!(stdin,"{}",json!({"jsonrpc":"2.0","id":i,"method":"tools/call","params":{"name":name,"arguments":args}})).unwrap();
            }
        }
        let o = c.wait_with_output().unwrap();
        assert!(o.status.success());
        String::from_utf8(o.stdout)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap()["result"].clone())
            .collect()
    }
}
impl Drop for Corpus {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn cbeta_citation_matches_returned_context_and_verified_quote() {
    let c = Corpus::new("citation");
    let lines = (1..=12)
        .map(|n| format!("<lb n=\"0001a{n:02}\"/>本文{n:02}\n"))
        .collect::<String>();
    c.put(
        "xml-p5/T/T01/T01n0001.xml",
        &format!("<TEI><text><body>\n{lines}</body></text></TEI>"),
    );
    let out = c.mcp(vec![
        ("fetch", json!({"id":"T01n0001","lb":"0001a04"})),
        (
            "fetch",
            json!({"id":"T01n0001","lb":"0001a04","contextBefore":0,"contextAfter":1}),
        ),
        ("fetch", json!({"id":"T01n0001","lineNumber":5})),
        ("fetch", json!({"ids":["T01n0001@0001a04","T01n0001:5"]})),
        (
            "verify",
            json!({"id":"T01n0001","quote":"本文04本文05","lb":"0001a04"}),
        ),
    ]);
    for (i, a, b) in [
        (0, "0001a02", "0001a09"),
        (1, "0001a04", "0001a05"),
        (2, "0001a02", "0001a09"),
    ] {
        let s = &out[i]["structuredContent"];
        assert_eq!(s["lbStart"], a, "{s}");
        assert_eq!(s["lbEnd"], b);
        assert_eq!(
            s["text"].as_str().unwrap().lines().count(),
            if i == 1 { 2 } else { 8 }
        );
        assert!(s["citation"].as_str().unwrap().ends_with(if i == 1 {
            "a04-05"
        } else {
            "a02-09"
        }));
    }
    for s in out[3]["structuredContent"]["results"].as_array().unwrap() {
        assert_eq!(s["lbStart"], "0001a02", "{s}");
        assert_eq!(s["lbEnd"], "0001a09");
        assert!(s["text"].as_str().unwrap().contains("本文09"));
        assert!(!s["text"].as_str().unwrap().contains("本文01"));
    }
    let s = &out[4]["structuredContent"];
    assert_eq!(s["status"], "exact");
    assert_eq!(s["location"]["lbStart"], "0001a04");
    assert_eq!(s["location"]["lbEnd"], "0001a05");
    assert!(s["citation"].as_str().unwrap().ends_with("a04-05"));
    let sliced = c.mcp(vec![
        (
            "fetch",
            json!({"id":"T01n0001","lb":"0001a04","maxChars":4}),
        ),
        (
            "fetch",
            json!({"id":"T01n0001","lb":"0001a04","startChar":5,"maxChars":4}),
        ),
        ("fetch", json!({"ids":["T01n0001@0001a04"],"maxChars":4})),
        (
            "fetch",
            json!({"ids":["T01n0001@0001a04"],"contextBefore":0,"contextAfter":1}),
        ),
    ]);
    assert_eq!(sliced[0]["structuredContent"]["lbEnd"], "0001a02");
    assert_eq!(sliced[1]["structuredContent"]["text"], "本文03");
    assert_eq!(sliced[1]["structuredContent"]["lbStart"], "0001a03");
    assert_eq!(sliced[1]["structuredContent"]["lbEnd"], "0001a03");
    assert_eq!(
        sliced[2]["structuredContent"]["results"][0]["lbEnd"],
        "0001a02"
    );
    assert_eq!(
        sliced[3]["structuredContent"]["results"][0]["lbStart"],
        "0001a04"
    );
    assert_eq!(
        sliced[3]["structuredContent"]["results"][0]["lbEnd"],
        "0001a05"
    );
}
#[test]
fn nonexistent_tipitaka_numbers_never_resolve_to_other_texts() {
    let c = Corpus::new("nikaya");
    for f in [
        "s0101m.mul",
        "s0103m.mul",
        "s0203m.mul",
        "s0301m.mul",
        "s0401m.mul",
        "s0501m.mul",
    ] {
        c.put(
            &format!("tipitaka-xml/romn/{f}.xml"),
            "<TEI><text><body><p>Real text</p></body></text></TEI>",
        );
    }
    for id in [
        "DN99",
        "DN0",
        "MN153",
        "SN99",
        "AN99",
        "KN99",
        "DN1.99",
        "SN1.99999",
        "AN1.99999",
    ] {
        for a in [
            vec!["--json", "tipitaka-fetch", "--id", id],
            vec!["--json", "fetch", id],
        ] {
            let o = c.run(&a);
            assert_eq!(
                o.status.code(),
                Some(10),
                "{id}: {}",
                String::from_utf8_lossy(&o.stdout)
            );
        }
    }
    c.put("tipitaka-xml/romn/s0301m.mul.xml", "<TEI><text><body><div type=\"samyutta\"><p rend=\"subhead\">1. Firstsuttaṃ</p><p>First</p></div><div type=\"samyutta\"><p rend=\"subhead\">1-2. Secondsuttāni</p><p>Second</p></div></body></text></TEI>");
    c.put("tipitaka-xml/romn/s0404m4.mul.xml", "<TEI><text><body><div id=\"an11\"><p rend=\"subhead\">1. Lastsuttaṃ</p><p>Last</p></div></body></text></TEI>");
    c.put(
        "tipitaka-xml/romn/s0519m.mul.xml",
        "<TEI><text><body><div id=\"kn19\"><p>Last</p></div></body></text></TEI>",
    );
    c.put(
        "tipitaka-xml/romn/s0518m.nrf.xml",
        "<TEI><text><body><div id=\"kn18\"><p>Milinda</p></div></body></text></TEI>",
    );
    for id in ["SN2", "SN2.2", "AN11", "AN11.1", "KN18", "KN19"] {
        assert!(
            c.run(&["tipitaka-fetch", "--id", id, "--json"])
                .status
                .success(),
            "{id}"
        );
    }
    for id in ["SN3", "SN2.3", "AN11.2", "KN20"] {
        assert_eq!(
            c.run(&["tipitaka-fetch", "--id", id, "--json"])
                .status
                .code(),
            Some(10),
            "{id}"
        );
    }
    let o = c.run(&["tipitaka-fetch", "--id", "DN99", "--json"]);
    let error: Value = serde_json::from_slice(&o.stderr).unwrap();
    assert_eq!(error["candidates"][0], "DN34");
    let out = c.mcp(vec![("fetch", json!({"id":"DN99"}))]);
    assert_eq!(out[0]["isError"], true);
    assert!(c
        .run(&["--json", "tipitaka-fetch", "--id", "DN34"])
        .status
        .success());
}
#[test]
fn shoku_missing_page_and_work_are_not_found() {
    let c = Corpus::new("shoku");
    c.put(
        "SHOKU_SHINZEN/kunten_tei_xml/volume_01_page_001.xml",
        "<TEI><text><body><p>本文</p></body></text></TEI>",
    );
    for id in ["SKZ99p9999", "SKZW9999"] {
        for a in [
            vec!["--json", "shoku-shinzen-fetch", "--id", id],
            vec!["--json", "fetch", id],
        ] {
            let o = c.run(&a);
            assert_eq!(
                o.status.code(),
                Some(10),
                "{}",
                String::from_utf8_lossy(&o.stdout)
            );
            let e: Value = serde_json::from_slice(&o.stderr).unwrap();
            assert_eq!(e["error"]["code"], "NOT_FOUND");
        }
    }
}
fn search_fixture(name: &str) -> Corpus {
    let c = Corpus::new(name);
    for n in 1..=5 {
        for p in [
            format!("xml-p5/T/T01/T01n{n:04}.xml"),
            format!("SHINZEN/kunten_tei_xml/volume_01_page_{n:03}.xml"),
        ] {
            c.put(
                &p,
                "<TEI><text><body>\n<p><lb n=\"0001a01\"/>六大無礙</p>\n</body></text></TEI>",
            );
        }
    }
    c
}
#[test]
fn cross_search_fetch_commands_match_corpus_search() {
    let c = search_fixture("commands");
    let all = c.run(&["--text", "search", "六大無礙", "--max-results", "3"]);
    let specific = c.run(&["--text", "cbeta-search", "六大無礙", "--max-results", "3"]);
    let fetches = |o: Output| {
        String::from_utf8(o.stdout)
            .unwrap()
            .lines()
            .filter_map(|l| l.trim().strip_prefix("fetch: ").map(str::to_owned))
            .collect::<Vec<_>>()
    };
    assert_eq!(fetches(all), fetches(specific));
    let o = c.run(&["--json", "search", "六大無礙", "--max-results", "3"]);
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    let cmd = v["result"]["structuredContent"]["corpora"][0]["results"][0]["nextAction"]["command"]
        .as_str()
        .unwrap();
    assert!(cmd.ends_with("--text"));
    assert!(!cmd.contains("highlight"));
}
#[test]
fn cross_search_continuations_use_displayed_page_size_without_gaps() {
    let c = search_fixture("paging");
    let replies = c.mcp(vec![("search", json!({"query":"六大無礙","maxResults":4}))]);
    for group in replies[0]["structuredContent"]["corpora"]
        .as_array()
        .unwrap()
    {
        let shown = group["results"].as_array().unwrap();
        let action = &group["nextAction"];
        assert_eq!(action["args"]["offset"], shown.len());
        assert_eq!(action["args"]["maxResults"], shown.len().max(1));
        let next = c.mcp(vec![("search", action["args"].clone())]);
        let ids = shown
            .iter()
            .map(|v| v["file_id"].clone())
            .collect::<Vec<_>>();
        let results = next[0]["_meta"]["results"].as_array().unwrap();
        assert!(results.iter().all(|v| !ids.contains(&v["file_id"])));
        assert_eq!(
            results[0]["file_id"],
            if group["source"] == "cbeta" {
                "T01n0004"
            } else {
                "SZ01p0002"
            }
        );
        let mut seen = ids;
        let mut offset = shown.len();
        loop {
            let page = c.mcp(vec![("search", json!({"source":group["source"], "query":"六大無礙", "maxResults":shown.len().max(1), "offset":offset}))]);
            let rows = page[0]["_meta"]["results"].as_array().unwrap();
            if rows.is_empty() {
                break;
            }
            for row in rows {
                assert!(!seen.contains(&row["file_id"]));
                seen.push(row["file_id"].clone());
            }
            offset += rows.len();
        }
        let command = format!(
            "{}-search",
            group["source"].as_str().unwrap().replace('_', "-")
        );
        let expected = c.run(&[&command, "六大無礙", "--max-results", "99", "--json"]);
        let expected: Value = serde_json::from_slice(&expected.stdout).unwrap();
        let expected: Vec<_> = expected["result"]["_meta"]["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["file_id"].clone())
            .collect();
        assert_eq!(seen, expected);
    }
}
#[test]
fn fetch_help_describes_fetch_and_schema_options() {
    let c = Corpus::new("help");
    let o = c.run(&["fetch", "--help"]);
    assert!(o.status.success());
    let s = String::from_utf8(o.stdout).unwrap();
    assert!(s.contains("Usage: buddha fetch"), "{s}");
    for flag in [
        "--lb",
        "--line-number",
        "--context-before",
        "--context-after",
        "--max-chars",
        "--start-char",
        "--text",
        "--json",
        "ID@lb",
        "ID:line",
    ] {
        assert!(s.contains(flag), "{flag}: {s}");
    }
    for path in [
        "xml-p5/T/T01/T01n0001.xml",
        "tipitaka-xml/romn/fixture.xml",
        "GRETIL/1_sanskr/tei/fixture.xml",
    ] {
        c.put(path, "<TEI><text><body><p>本文</p></body></text></TEI>");
    }
    c.put(
        "SHOKU_SHINZEN/kunten_tei_xml/volume_01_page_001.xml",
        "<TEI><text><p>本文</p></text></TEI>",
    );
    c.put(
        "SHINZEN/kunten_tei_xml/volume_01_page_001.xml",
        "<TEI><text><p>本文</p></text></TEI>",
    );
    for id in [
        "__buddha_missing__",
        "T99n9999",
        "DN99",
        "sa___missing__",
        "SZ99p9999",
        "SKZ99p9999",
        "SKZW9999",
    ] {
        let missing = c.run(&["fetch", id, "--json"]);
        assert_eq!(
            missing.status.code(),
            Some(10),
            "{id}: {}",
            String::from_utf8_lossy(&missing.stdout)
        );
    }
    let schema = c.run(&["schema", "--command", "fetch", "--json"]);
    let schema: Value = serde_json::from_slice(&schema.stdout).unwrap();
    for flag in [
        "--lb",
        "--line-number",
        "--context-before",
        "--context-after",
        "--max-chars",
        "--start-char",
        "--text",
        "--json",
    ] {
        assert!(
            schema["commands"][0]["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["flag"] == flag),
            "missing schema flag: {flag}"
        );
    }
    let o = c.run(&["search", "--help"]);
    assert!(o.status.success());
    assert!(String::from_utf8(o.stdout)
        .unwrap()
        .contains("Usage: buddha search"));
}
