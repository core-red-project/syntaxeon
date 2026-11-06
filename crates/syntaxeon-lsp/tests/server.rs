#![allow(clippy::unwrap_used, clippy::expect_used)] // integration tests: a panic is the failure report
#![allow(clippy::assert_is_empty)] // `is_empty` reads better than comparing with `[]` for JSON arrays
#![allow(clippy::cast_possible_truncation)] // small test positions always fit
//! The server driven through an in-memory connection, as an editor would.

use lsp_server::{Connection, Message, Notification, Request, RequestId};
use serde_json::{Value, json};

struct Client {
    conn: Connection,
    next: i32,
    /// Notifications that arrived while waiting for something else.
    pending: Vec<Notification>,
    server: Option<std::thread::JoinHandle<()>>,
}

impl Client {
    fn start() -> Client {
        let (server, conn) = Connection::memory();
        let handle = std::thread::spawn(move || syntaxeon_lsp::run(&server).unwrap());
        let mut c = Client {
            conn,
            next: 0,
            pending: Vec::new(),
            server: Some(handle),
        };
        let init = c.request("initialize", json!({"capabilities": {}}));
        assert!(init["capabilities"]["hoverProvider"].as_bool().unwrap());
        c.notify("initialized", json!({}));
        c
    }

    fn notify(&self, method: &str, params: Value) {
        self.conn
            .sender
            .send(Message::Notification(Notification::new(
                method.to_string(),
                params,
            )))
            .unwrap();
    }

    /// The `result` of a request, or its `error` object.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        self.next += 1;
        let id = RequestId::from(self.next);
        self.conn
            .sender
            .send(Message::Request(Request::new(
                id.clone(),
                method.to_string(),
                params,
            )))
            .unwrap();
        loop {
            match self.conn.receiver.recv().unwrap() {
                Message::Response(r) if r.id == id => {
                    return match r.response_result {
                        Err(e) => Err(json!({"code": e.code, "message": e.message})),
                        Ok(value) => Ok(value),
                    };
                }
                Message::Notification(n) => self.pending.push(n),
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params).unwrap()
    }

    /// Opens (or replaces) a document and returns the diagnostics the server publishes for it.
    fn open(&mut self, uri: &str, text: &str, change: bool) -> Vec<Value> {
        if change {
            self.notify(
                "textDocument/didChange",
                json!({"textDocument": {"uri": uri, "version": 2}, "contentChanges": [{"text": text}]}),
            );
        } else {
            self.notify(
                "textDocument/didOpen",
                json!({"textDocument": {"uri": uri, "languageId": "syntaxeon", "version": 1, "text": text}}),
            );
        }
        loop {
            let n = match self.pending.pop() {
                Some(n) => n,
                None => match self.conn.receiver.recv().unwrap() {
                    Message::Notification(n) => n,
                    other => panic!("unexpected {other:?}"),
                },
            };
            if n.method == "textDocument/publishDiagnostics" && n.params["uri"] == uri {
                return n.params["diagnostics"].as_array().unwrap().clone();
            }
        }
    }

    fn at(&mut self, method: &str, uri: &str, line: u32, character: u32) -> Value {
        self.request(
            method,
            json!({"textDocument": {"uri": uri}, "position": {"line": line, "character": character}}),
        )
    }

    fn stop(mut self) {
        self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        self.server.take().unwrap().join().unwrap();
    }
}

const URI: &str = "file:///nowhere/tale.sxe";
const GOOD: &str = "Thus begins the tale of men:\nSpeak \"Hello again\".\nProclaim the text of ten.\nSpeak \"Amen\".\n";
const BAD: &str = "Thus begins the tale of men:\nSpeak \"Hello again\".\nProclaim the text of ten.\nSpeak \"Goodbye\".\n";

#[test]
fn a_valid_program_has_no_diagnostics_and_a_broken_rhyme_is_underlined() {
    let mut c = Client::start();
    assert!(c.open(URI, GOOD, false).is_empty());
    let d = c.open(URI, BAD, true);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0]["code"], "SXP-202");
    assert_eq!(d[0]["source"], "syntaxeon");
    assert_eq!(d[0]["severity"], 1);
    assert!(
        d[0]["message"]
            .as_str()
            .unwrap()
            .starts_with("The verse hath failed to rhyme.\n")
    );
    // `"Goodbye"` on line 3, columns 6 to 15.
    assert_eq!(
        d[0]["range"],
        json!({"start": {"line": 3, "character": 6}, "end": {"line": 3, "character": 15}})
    );
    c.stop();
}

#[test]
fn errors_that_know_only_a_line_underline_the_line() {
    let mut c = Client::start();
    let d = c.open(URI, "", false);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0]["code"], "SXP-408");
    c.stop();
}

#[test]
fn hover_tells_the_sound_the_group_and_the_relation() {
    let mut c = Client::start();
    c.open(URI, GOOD, false);
    // On `again` (line 1, inside the quotes): the second verse of group A, a perfect rhyme.
    let h = c.at("textDocument/hover", URI, 1, 10);
    let text = h["contents"]["value"].as_str().unwrap();
    assert!(
        text.contains("**again**") && text.contains("`AH0 G EH1 N`"),
        "{text}"
    );
    assert!(
        text.contains("group **A**") && text.contains("Perfect rhyme with line 1 (`men`)"),
        "{text}"
    );
    // On `Amen`: the second verse of group B, against `ten`.
    let h = c.at("textDocument/hover", URI, 3, 10);
    let text = h["contents"]["value"].as_str().unwrap();
    assert!(
        text.contains("group **B**") && text.contains("with line 3 (`ten`)"),
        "{text}"
    );
    // The first verse of a group has no leader, so no relation.
    let h = c.at("textDocument/hover", URI, 2, 22);
    assert!(
        !h["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("rhyme with"),
        "{h}"
    );
    // Elsewhere there is nothing to say.
    assert_eq!(c.at("textDocument/hover", URI, 0, 0), Value::Null);
    c.stop();
}

#[test]
fn formatting_returns_one_edit_only_when_something_changes() {
    let mut c = Client::start();
    let params = |uri: &str| json!({"textDocument": {"uri": uri}, "options": {"tabSize": 4, "insertSpaces": true}});
    c.open(URI, GOOD, false);
    assert_eq!(c.request("textDocument/formatting", params(URI)), json!([]));
    let messy = "Thus begins the tale of men:   \nSpeak \"Hello again\".  \nProclaim the text of ten.\nSpeak \"Amen\".\n\n\n";
    c.open(URI, messy, true);
    let edits = c.request("textDocument/formatting", params(URI));
    assert_eq!(edits.as_array().unwrap().len(), 1);
    assert!(!edits[0]["newText"].as_str().unwrap().contains("   \n"));
    c.stop();
}

#[test]
fn a_rhyme_quick_fix_really_fixes_the_program() {
    let mut c = Client::start();
    c.open(URI, BAD, false);
    let actions = c.request(
        "textDocument/codeAction",
        json!({
            "textDocument": {"uri": URI},
            "range": {"start": {"line": 3, "character": 8}, "end": {"line": 3, "character": 8}},
            "context": {"diagnostics": []}
        }),
    );
    let actions = actions.as_array().unwrap();
    assert!(!actions.is_empty() && actions.len() <= 6, "{actions:?}");
    for a in actions {
        assert_eq!(a["kind"], "quickfix");
        let edit = &a["edit"]["changes"][URI][0];
        // Apply the one-line edit by hand and check the result with the server itself.
        let range = &edit["range"];
        assert_eq!(range["start"]["line"], 3);
        let (from, to) = (
            range["start"]["character"].as_u64().unwrap() as usize,
            range["end"]["character"].as_u64().unwrap() as usize,
        );
        let mut lines: Vec<String> = BAD.lines().map(String::from).collect();
        lines[3].replace_range(from..to, edit["newText"].as_str().unwrap());
        let fixed = lines.join("\n") + "\n";
        assert!(
            c.open(URI, &fixed, true).is_empty(),
            "{}: {fixed}",
            a["title"]
        );
        c.open(URI, BAD, true);
    }
    c.stop();
}

#[test]
fn end_words_are_coloured_by_rhyme_group() {
    let mut c = Client::start();
    c.open(URI, BAD, false);
    let t = c.request(
        "textDocument/semanticTokens/full",
        json!({"textDocument": {"uri": URI}}),
    );
    let data: Vec<u64> = t["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap())
        .collect();
    // Five numbers per token: delta line, delta start, length, type, modifiers.
    let tokens: Vec<&[u64]> = data.chunks(5).collect();
    assert_eq!(tokens.len(), 4);
    // Verses 0 and 1 are group A (type 0), verses 2 and 3 group B (type 1); the last breaks it.
    assert_eq!(
        [tokens[0][3], tokens[1][3], tokens[2][3], tokens[3][3]],
        [0, 0, 1, 1]
    );
    assert_eq!(
        [tokens[0][4], tokens[1][4], tokens[2][4], tokens[3][4]],
        [0, 0, 0, 1]
    );
    c.stop();
}

#[test]
fn the_verse_report_and_the_comparison_feed_an_audio_layer() {
    let mut c = Client::start();
    c.open(URI, GOOD, false);
    let r = c.request("syntaxeon/verses", json!({"textDocument": {"uri": URI}}));
    let stanza = &r[0];
    assert_eq!(stanza["scheme"], "AABB");
    assert_eq!(stanza["valid"], true);
    let v = &stanza["verses"][1];
    assert_eq!(v["endWord"], "again");
    assert_eq!(v["group"], "A");
    assert_eq!(v["leader"], 0);
    assert_eq!(v["kind"], "perfect");
    assert_eq!(v["pronunciations"].as_array().unwrap().len(), 2);
    assert_eq!(v["line"], 1);

    let cmp = c.request("syntaxeon/compare", json!({"a": "Tonight", "b": "light"}));
    assert_eq!(cmp["kind"], "perfect");
    assert_eq!(cmp["rhymes"], true);
    assert_eq!(cmp["a"]["pronunciations"][0], "T AH0 N AY1 T");
    assert_eq!(
        c.request("syntaxeon/compare", json!({"a": "men", "b": "bed"}))["kind"],
        "assonant"
    );
    let err = c
        .call("syntaxeon/compare", json!({"a": "men", "b": "zyzzyva"}))
        .unwrap_err();
    assert_eq!(err["code"], -32602);
    c.stop();
}

#[test]
fn unknown_requests_and_closed_documents_are_refused() {
    let mut c = Client::start();
    assert_eq!(
        c.call("textDocument/typeDefinition", json!({}))
            .unwrap_err()["code"],
        -32601
    );
    c.open(URI, GOOD, false);
    c.notify(
        "textDocument/didClose",
        json!({"textDocument": {"uri": URI}}),
    );
    // Closing clears the diagnostics.
    loop {
        let m = c.conn.receiver.recv().unwrap();
        if let Message::Notification(n) = m
            && n.method == "textDocument/publishDiagnostics"
        {
            assert!(n.params["diagnostics"].as_array().unwrap().is_empty());
            break;
        }
    }
    let err = c
        .call(
            "textDocument/hover",
            json!({"textDocument": {"uri": URI}, "position": {"line": 0, "character": 0}}),
        )
        .unwrap_err();
    assert_eq!(err["code"], -32602);
    c.stop();
}

#[test]
fn the_rhythm_file_next_to_the_document_sets_the_rules() {
    let dir = std::env::temp_dir().join(format!("syntaxeon-lsp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let uri = format!("file://{}/tale.sxe", dir.display());
    let assonant = "Thus begins the tale of men:\nSpeak \"Go to bed\".\n";
    let mut c = Client::start();
    assert!(c.open(&uri, assonant, false).is_empty());
    std::fs::write(dir.join("rhythm.toml"), "[rhyme]\nassonance = false\n").unwrap();
    let d = c.open(&uri, assonant, true);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0]["code"], "SXP-202");
    // The hover says why.
    let h = c.at("textDocument/hover", &uri, 1, 10);
    assert!(
        h["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("Assonance only"),
        "{h}"
    );
    // A broken file is reported, not ignored.
    std::fs::write(dir.join("rhythm.toml"), "[rhyme]\nassonanse = false\n").unwrap();
    let d = c.open(&uri, assonant, true);
    assert!(d.iter().any(|x| x["code"] == "SXP-305"), "{d:?}");
    c.stop();
    std::fs::remove_dir_all(&dir).unwrap();
}

const CALLS: &str = "Thus begins the tale of men:\nProclaim the text of calling twice with ten.\n\nTo twice, taking price as whole number, giving whole number:\nLet total be the sum of the price and the price.\nGive back total.\n";

fn range_of(v: &Value) -> ((u64, u64), (u64, u64)) {
    let p = |k: &str| {
        (
            v[k]["line"].as_u64().unwrap(),
            v[k]["character"].as_u64().unwrap(),
        )
    };
    (p("start"), p("end"))
}

#[test]
fn definition_leads_from_a_use_to_its_declaration() {
    let mut c = Client::start();
    c.open(URI, CALLS, false);
    // The call `twice` on line 2 leads to the title on line 4.
    let call = c.at("textDocument/definition", URI, 1, 31);
    assert_eq!(call["uri"], URI);
    assert_eq!(range_of(&call["range"]), ((3, 3), (3, 8)));
    // `total` in `Give back total.` leads to its declaration, `price` to the parameter.
    let total = c.at("textDocument/definition", URI, 5, 12);
    assert_eq!(range_of(&total["range"]), ((4, 4), (4, 9)));
    let price = c.at("textDocument/definition", URI, 4, 30);
    assert_eq!(range_of(&price["range"]), ((3, 17), (3, 22)));
    // A word that is no name leads nowhere.
    assert!(c.at("textDocument/definition", URI, 1, 2).is_null());
    c.stop();
}

#[test]
fn completion_offers_names_in_reach_and_rhyming_end_words() {
    let mut c = Client::start();
    c.open(URI, CALLS, false);
    let labels = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|i| i["label"].as_str().unwrap().to_string())
            .collect()
    };
    // After the declaration of `total`: it, the parameter and both functions are offered.
    let names = labels(&c.at("textDocument/completion", URI, 5, 5));
    for want in ["total", "price", "twice", "tale of men"] {
        assert!(
            names.iter().any(|n| n == want),
            "{want} missing from {names:?}"
        );
    }
    // Before the declaration, `total` is not yet there.
    let early = labels(&c.at("textDocument/completion", URI, 4, 4));
    assert!(!early.iter().any(|n| n == "total"));
    // On the end word of a verse that must rhyme, words that rhyme with the leader come first,
    // as an edit of the end word.
    c.open(URI, BAD, true);
    let items = c.at("textDocument/completion", URI, 3, 9);
    let first = &items.as_array().unwrap()[0];
    assert!(first["detail"].as_str().unwrap().starts_with("rhymes with"));
    assert_eq!(first["textEdit"]["range"]["start"]["line"], 3);
    c.stop();
}

#[test]
fn references_and_rename_follow_a_name_everywhere() {
    let mut c = Client::start();
    c.open(URI, CALLS, false);
    let refs = |c: &mut Client, decl: bool| {
        c.request(
            "textDocument/references",
            json!({"textDocument": {"uri": URI}, "position": {"line": 5, "character": 12},
                   "context": {"includeDeclaration": decl}}),
        )
    };
    assert_eq!(refs(&mut c, true).as_array().unwrap().len(), 2);
    assert_eq!(refs(&mut c, false).as_array().unwrap().len(), 1);
    let prep = c.at("textDocument/prepareRename", URI, 5, 12);
    assert_eq!(range_of(&prep), ((5, 10), (5, 15)));
    let rename = |c: &mut Client, name: &str| {
        c.call(
            "textDocument/rename",
            json!({"textDocument": {"uri": URI}, "position": {"line": 5, "character": 12}, "newName": name}),
        )
    };
    // A parameter that appears twice, plus its declaration: renamed everywhere.
    let ok = c
        .call(
            "textDocument/rename",
            json!({"textDocument": {"uri": URI}, "position": {"line": 4, "character": 30}, "newName": "cost"}),
        )
        .unwrap();
    assert_eq!(ok["changes"][URI].as_array().unwrap().len(), 3);
    // Not a name, a reserved word and a name that breaks the rhyme are refused.
    assert!(rename(&mut c, "").is_err());
    assert!(rename(&mut c, "x1").is_err());
    assert!(rename(&mut c, "speak").is_err());
    c.stop();
}
