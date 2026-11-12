use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn local_search_pages_cli_and_mcp() {
    let root = std::env::temp_dir().join(format!("buddha-offset-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    for n in 1..=5 {
        for path in [
            format!("xml-p5/T/T01/T01n{n:04}.xml"),
            format!("GRETIL/1_sanskr/tei/file{n}.xml"),
            format!("SARIT-corpus/file{n}.xml"),
            format!("MUKTABODHA/file{n}.xml"),
            format!("tipitaka-xml/romn/file{n}.xml"),
            format!("SHINZEN/kunten_tei_xml/volume_01_page_{n:03}.xml"),
            format!("SHOKU_SHINZEN/kunten_tei_xml/volume_01_page_{n:03}.xml"),
        ] {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                path,
                "<TEI><text><body><p><lb n=\"0\"/>dharma</p></body></text></TEI>",
            )
            .unwrap();
        }
    }
    let mut expected_pages = HashMap::new();
    for source in [
        "cbeta",
        "tipitaka",
        "gretil",
        "sarit",
        "muktabodha",
        "shinzen",
        "shoku-shinzen",
    ] {
        let mut pages = Vec::new();
        for offset in [0, 2, 4, 5, 99] {
            let out = Command::new(env!("CARGO_BIN_EXE_buddha"))
                .env("BUDDHA_DIR", &root)
                .args([
                    "--json",
                    &format!("{source}-search"),
                    "--query",
                    "dharma",
                    "--max-results",
                    "2",
                    "--offset",
                    &offset.to_string(),
                ])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{source}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let reply: Value = serde_json::from_slice(&out.stdout).unwrap();
            let meta = &reply["result"]["_meta"];
            assert_eq!(meta["totalFilesMatched"], 5, "{reply}");
            if offset >= 5 {
                assert_eq!(meta["totalFiles"], 0);
                assert!(meta["warnings"].to_string().contains(&offset.to_string()));
                assert!(meta.get("nextOffset").is_none());
            } else {
                if offset == 4 {
                    assert!(meta.get("nextOffset").is_none());
                } else {
                    assert_eq!(meta["nextOffset"], offset + 2);
                }
                pages.push(
                    meta["results"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v["file_id"].as_str().unwrap().to_owned())
                        .collect::<Vec<_>>(),
                );
            }
        }
        assert!(pages[0].iter().all(|id| !pages[1].contains(id)));
        assert!(pages[0][1] < pages[1][0]);
        expected_pages.insert(source.replace('-', "_"), pages.concat());
        let schema = Command::new(env!("CARGO_BIN_EXE_buddha"))
            .args(["schema", "--command", &format!("{source}-search")])
            .output()
            .unwrap();
        let schema: Value = serde_json::from_slice(&schema.stdout).unwrap();
        let arg = schema["commands"][0]["args"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == "offset")
            .unwrap();
        assert_eq!(arg["valueType"], "integer");
        if matches!(source, "cbeta" | "gretil" | "shinzen" | "shoku-shinzen") {
            for offset in [0, 2, 5] {
                let out = Command::new(env!("CARGO_BIN_EXE_buddha"))
                    .env("BUDDHA_DIR", &root)
                    .args([
                        "--json",
                        &format!("{source}-pipeline"),
                        "--query",
                        "dharma",
                        "--max-results",
                        "2",
                        "--offset",
                        &offset.to_string(),
                    ])
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                let reply: Value = serde_json::from_slice(&out.stdout).unwrap();
                let meta = &reply["result"]["_meta"];
                assert_eq!(meta["totalFilesMatched"], 5);
                if offset < 5 {
                    assert_eq!(meta["nextOffset"], offset + 2);
                    assert_eq!(
                        meta["results"][0]["file_id"],
                        expected_pages[&source.replace('-', "_")][offset]
                    );
                    assert_eq!(
                        reply["result"]["structuredContent"]["nextOffset"],
                        offset + 2
                    );
                } else {
                    assert_eq!(meta["totalFiles"], 0);
                }
            }
        }
    }
    for legacy in [false, true] {
        let mut requests = vec![json!({"jsonrpc":"2.0","id":0,"method":"tools/list"})];
        for source in [
            "cbeta",
            "tipitaka",
            "gretil",
            "sarit",
            "muktabodha",
            "shinzen",
            "shoku_shinzen",
        ] {
            for kind in ["search", "pipeline"] {
                if kind == "pipeline" && matches!(source, "tipitaka") {
                    continue;
                }
                for offset in [0, 2, 4, 5, 99] {
                    requests.push(json!({"jsonrpc":"2.0","id":requests.len(),"method":"tools/call","params":{"name":if legacy {format!("{source}_{kind}")} else {kind.into()},"arguments":{"source":source,"query":"dharma","maxResults":2,"offset":offset,"autoFetch":false}}}));
                }
            }
            requests.push(json!({"jsonrpc":"2.0","id":requests.len(),"method":"tools/call","params":{"name":if legacy {format!("{source}_search")} else {"search".into()},"arguments":{"source":source,"query":"dharma","maxResults":2,"offs":2}}}));
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_buddha"))
            .env("BUDDHA_DIR", &root)
            .env("BUDDHA_UNIFIED_TOOLS", if legacy { "0" } else { "1" })
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        {
            let mut stdin = child.stdin.take().unwrap();
            for req in &requests {
                writeln!(stdin, "{req}").unwrap();
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
        for tool in replies[0]["result"]["tools"].as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            if matches!(name, "search" | "pipeline")
                || name.ends_with("_search")
                    && !name.ends_with("title_search")
                    && !name.starts_with("sat")
                    && !name.starts_with("jozen")
                    && !name.starts_with("tibetan")
            {
                assert_eq!(
                    tool["inputSchema"]["properties"]["offset"]["type"], "integer",
                    "{name}"
                );
            }
        }
        for (req, reply) in requests.iter().zip(&replies).skip(1) {
            let args = &req["params"]["arguments"];
            let offset = args
                .get("offset")
                .or_else(|| args.get("offs"))
                .unwrap()
                .as_u64()
                .unwrap();
            let meta = &reply["result"]["_meta"];
            assert_eq!(meta["totalFilesMatched"], 5, "{req}: {reply}");
            assert_eq!(
                meta["totalFiles"],
                if offset >= 5 {
                    0
                } else if offset == 4 {
                    1
                } else {
                    2
                },
                "{req}: {reply}"
            );
            if offset >= 5 {
                assert!(meta["warnings"].to_string().contains(&offset.to_string()));
                assert!(meta.get("nextOffset").is_none());
            } else if offset == 4 {
                assert!(meta.get("nextOffset").is_none());
                assert!(meta.get("nextActions").and_then(Value::as_array).map_or(
                    true,
                    |actions| actions.iter().all(|a| a["tool"] != "search"
                        && !a["tool"].as_str().unwrap_or("").ends_with("_search"))
                ));
            } else if offset < 4 {
                assert_eq!(meta["nextOffset"], offset + 2);
                let last = meta["nextActions"].as_array().unwrap().last().unwrap();
                assert_eq!(last["args"]["offset"], offset + 2, "{reply}");
                assert!(last["tool"].as_str().unwrap().ends_with("search"));
                assert_eq!(last["args"]["query"], "dharma");
                assert_eq!(
                    reply["result"]["structuredContent"]["nextOffset"],
                    offset + 2
                );
                assert_eq!(reply["result"]["structuredContent"]["totalFilesMatched"], 5);
                assert_eq!(
                    last["args"]["maxMatchesPerFile"],
                    if req["params"]["name"]
                        .as_str()
                        .unwrap()
                        .ends_with("pipeline")
                    {
                        3
                    } else {
                        5
                    }
                );
                let ids: Vec<_> = meta["results"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r["file_id"].as_str().unwrap())
                    .collect();
                let expected = &expected_pages[args["source"].as_str().unwrap()];
                assert_eq!(ids, expected[offset as usize..offset as usize + 2]);
            }
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cbeta_fetch_omits_nested_mulu_but_keeps_body_heading() {
    let root = std::env::temp_dir().join(format!("buddha-mulu-{}", std::process::id()));
    let path = root.join("xml-p5/T/T01/T01n0001.xml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "<TEI xmlns:cb=\"urn:cb\"><text><body><cb:mulu><hi>No. 1217-A</hi></cb:mulu><head>No. 1217-A</head><p><lb n=\"0001a01\"/>本文</p></body></text></TEI>").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buddha"))
        .env("BUDDHA_DIR", &root)
        .args(["--json", "cbeta-fetch", "--id", "T01n0001", "--full"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let reply: Value = serde_json::from_slice(&out.stdout).unwrap();
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(text.matches("No. 1217-A").count(), 1, "{text}");
    assert!(text.contains("本文"));
    fs::remove_dir_all(root).unwrap();
}
