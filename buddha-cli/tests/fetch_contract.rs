use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

static NEXT_CORPUS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct Corpus(PathBuf);

impl Corpus {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "buddha-fetch-{}-{}",
            std::process::id(),
            NEXT_CORPUS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        for path in [
            "xml-p5/T/T01/T01n0001.xml",
            "GRETIL/1_sanskr/tei/fixture.xml",
            "SARIT-corpus/fixture.xml",
            "MUKTABODHA/fixture.xml",
            "tipitaka-xml/romn/fixture.xml",
        ] {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "<TEI><teiHeader><fileDesc><titleStmt><title>First</title></titleStmt></fileDesc></teiHeader><text><body><p>布施波羅蜜ā🙂ABC</p></body></text></TEI>").unwrap();
        }
        let other = root.join("xml-p5/T/T01/T01n0002.xml");
        fs::write(other, "<TEI><teiHeader><fileDesc><titleStmt><title>OtherUniqueTitle</title></titleStmt></fileDesc></teiHeader><text><body><p>別資料</p></body></text></TEI>").unwrap();
        Self(root)
    }

    fn run(&self, corpus: &str, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_buddha"))
            .env("BUDDHA_DIR", &self.0)
            .arg("--json")
            .arg(format!("{corpus}-fetch"))
            .args(args)
            .output()
            .unwrap()
    }

    fn fetch(&self, corpus: &str, args: &[&str]) -> Value {
        let out = self.run(corpus, args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["result"].clone()
    }
}

impl Drop for Corpus {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn local_fetch_preserves_text_cursors_and_source_identity() {
    let fixture = Corpus::new();
    for corpus in ["cbeta", "gretil", "sarit", "muktabodha", "tipitaka"] {
        let id = if corpus == "cbeta" {
            "T01n0001"
        } else {
            "fixture"
        };
        let invalid = fixture.run(corpus, &["--id", id, "--line-number", "999"]);
        assert_eq!(invalid.status.code(), Some(2), "{corpus}");
        let error: Value = serde_json::from_slice(&invalid.stderr).unwrap();
        assert_eq!(error["error"]["code"], "INVALID_ARGUMENT", "{corpus}");
        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("1..=1"));
        let all = fixture.fetch(corpus, &["--id", id, "--max-chars", "1000"]);
        let text = all["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("布施波羅蜜"));
        if corpus == "cbeta" {
            assert!(!text.contains("First"));
        }
        let total = text.chars().count();
        assert_eq!(all["_meta"]["totalLength"], total);
        assert!(all["_meta"].get("nextStartChar").is_none());

        let mut reconstructed = String::new();
        let mut start = 0;
        loop {
            let page = fixture.fetch(
                corpus,
                &[
                    "--id",
                    id,
                    "--start-char",
                    &start.to_string(),
                    "--max-chars",
                    "3",
                ],
            );
            let part = page["content"][0]["text"].as_str().unwrap();
            assert_eq!(page["_meta"]["returnedStart"], start);
            let end = start + part.chars().count();
            assert_eq!(page["_meta"]["returnedEnd"], end);
            reconstructed.push_str(part);
            match page["_meta"]["nextStartChar"].as_u64() {
                Some(next) => {
                    assert!(next as usize > start && (next as usize) < total);
                    start = next as usize;
                }
                None => break,
            }
        }
        assert_eq!(reconstructed, text, "{corpus}");

        let plain = fixture.fetch(corpus, &["--id", id, "--max-chars", "4"]);
        let highlighted = fixture.fetch(
            corpus,
            &["--id", id, "--max-chars", "4", "--highlight", "布施"],
        );
        for key in [
            "totalLength",
            "returnedStart",
            "returnedEnd",
            "nextStartChar",
            "truncated",
        ] {
            assert_eq!(
                plain["_meta"][key], highlighted["_meta"][key],
                "{corpus}/{key}"
            );
        }
        let empty = fixture.fetch(corpus, &["--id", id, "--start-char", "999"]);
        assert_eq!(empty["content"][0]["text"], "");
        assert_eq!(empty["_meta"]["returnedStart"], total);
        assert_eq!(empty["_meta"]["returnedEnd"], total);
        assert!(empty["_meta"].get("nextStartChar").is_none());

        if corpus != "tipitaka" {
            let full = fixture.fetch(
                corpus,
                &[
                    "--id",
                    id,
                    "--full",
                    "--start-char",
                    "3",
                    "--max-chars",
                    "1",
                ],
            );
            assert_eq!(full["content"][0]["text"], text);
            assert_eq!(full["_meta"]["returnedStart"], 0);
            assert_eq!(full["_meta"]["returnedEnd"], total);
            assert_eq!(full["_meta"]["truncated"], false);
        }
        for (args, code, label) in [
            (vec!["--id", "__missing__"], 10, "NOT_FOUND"),
            (vec![], 2, "INVALID_ARGUMENT"),
            (
                vec!["--id", id, "--max-chars", "abc"],
                2,
                "INVALID_ARGUMENT",
            ),
        ] {
            let out = fixture.run(corpus, &args);
            assert_eq!(out.status.code(), Some(code), "{corpus}");
            let error: Value = serde_json::from_slice(&out.stderr).unwrap();
            assert_eq!(error["error"]["code"], label);
        }
    }

    let by_id = fixture.fetch("cbeta", &["--id", "T01n0001"]);
    let both = fixture.fetch(
        "cbeta",
        &["--id", "T01n0001", "--query", "OtherUniqueTitle"],
    );
    assert_eq!(by_id["content"], both["content"]);
    assert_eq!(both["_meta"]["matchedId"], "T01n0001");
    assert_ne!(both["_meta"]["matchedTitle"], "OtherUniqueTitle");

    fs::write(fixture.0.join("xml-p5/T/T01/T01n0003.xml"), r##"<TEI>
<teiHeader><encodingDesc><charDecl><char xml:id="CB1"><mapping type="unicode">佛</mapping></char></charDecl></encodingDesc></teiHeader>
<text><body>
<p>甲<g ref="#CB1"/>乙<note>注釈</note></p>
</body></text></TEI>"##).unwrap();
    let context = fixture.fetch(
        "cbeta",
        &[
            "--id",
            "T01n0003",
            "--line-number",
            "4",
            "--context-before",
            "0",
            "--context-after",
            "0",
        ],
    );
    assert_eq!(context["content"][0]["text"], "甲佛乙");
    let notes = fixture.fetch("cbeta", &["--id", "T01n0003", "--include-notes"]);
    assert!(notes["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("注釈"));

    let help = fixture.run("cbeta", &["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage:"));
}

#[test]
fn invalid_search_regex_is_an_argument_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_buddha"))
        .env(
            "BUDDHA_DIR",
            std::env::temp_dir().join("buddha-invalid-regex-no-corpus"),
        )
        .args(["--json", "cbeta-search", "--query", "["])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let error: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(error["error"]["code"], "INVALID_ARGUMENT");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("regex"));
}

#[test]
fn regex_search_reports_linewise_mode() {
    let corpus = Corpus::new();
    let out = Command::new(env!("CARGO_BIN_EXE_buddha"))
        .env("BUDDHA_DIR", &corpus.0)
        .args(["--json", "cbeta-search", "--query", "a(b)"])
        .output()
        .unwrap();
    let output: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(output["result"]["_meta"]["queryMode"], "regex");
    assert!(output["result"]["_meta"]["warnings"].as_array().unwrap()[0]
        .as_str()
        .unwrap()
        .contains("line"));
}
