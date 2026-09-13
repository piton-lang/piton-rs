//! The server tested through JSON-RPC, the way an editor talks to it.
//!
//! The feature tests in `tests.rs` check what each feature computes. These
//! check what an editor actually receives: the capabilities it is told about,
//! the errors it is shown, the diagnostics it is sent and when, and the edits
//! it applies, in the order real editors send their messages.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use piton_core::framework::Frameworks;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tower_lsp::jsonrpc::{Request, Response};
use tower_lsp::lsp_types::Url;
use tower_lsp::LspService;
use tower_service::Service;

use crate::line_index::LineIndex;
use crate::Backend;

/// A project on disk that no other test shares.
fn project(files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let ordinal = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("piton-lsp-protocol-{}-{ordinal}", std::process::id()));
    for (path, contents) in files {
        let target = root.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, contents).unwrap();
    }
    root
}

fn uri(path: &Path) -> String {
    Url::from_file_path(path).unwrap().to_string()
}

/// An editor connected to a fresh server.
struct Editor {
    service: LspService<Backend>,
    notifications: mpsc::UnboundedReceiver<Request>,
    next: i64,
    capabilities: Value,
}

impl Editor {
    async fn open(root: &Path) -> Editor {
        let (service, socket) = crate::service(Frameworks::default as fn() -> Frameworks);
        let (sender, notifications) = mpsc::unbounded_channel();
        let (mut requests, mut replies) = socket.split();
        // Answer what the server asks the editor, such as registering a
        // capability, and keep what it tells the editor for the test to read.
        tokio::spawn(async move {
            while let Some(request) = requests.next().await {
                match request.id().cloned() {
                    Some(id) => {
                        let _ = replies.send(Response::from_ok(id, Value::Null)).await;
                    }
                    None => {
                        let _ = sender.send(request);
                    }
                }
            }
        });
        let mut editor = Editor { service, notifications, next: 1, capabilities: Value::Null };
        let initialized = editor
            .request(
                "initialize",
                json!({ "capabilities": {}, "workspaceFolders": [{ "uri": uri(root), "name": "test" }] }),
            )
            .await
            .expect("the server initialises");
        editor.capabilities = initialized["capabilities"].clone();
        editor.notify("initialized", json!({})).await;
        editor
    }

    async fn call(&mut self, request: Request) -> Option<Response> {
        std::future::poll_fn(|cx| self.service.poll_ready(cx)).await.expect("the server is running");
        self.service.call(request).await.expect("the server is running")
    }

    async fn request(&mut self, method: &'static str, params: Value) -> Result<Value, tower_lsp::jsonrpc::Error> {
        let id = self.next;
        self.next += 1;
        let response = self
            .call(Request::build(method).id(id).params(params).finish())
            .await
            .expect("a request is answered");
        response.into_parts().1
    }

    async fn notify(&mut self, method: &'static str, params: Value) {
        self.call(Request::build(method).params(params).finish()).await;
    }

    /// The next diagnostics published for `path`.
    async fn diagnostics(&mut self, path: &Path) -> Vec<Value> {
        loop {
            let request = tokio::time::timeout(Duration::from_secs(10), self.notifications.recv())
                .await
                .expect("diagnostics are published")
                .expect("the server is running");
            if request.method() != "textDocument/publishDiagnostics" {
                continue;
            }
            let params = request.params().cloned().unwrap_or_default();
            if params["uri"] == uri(path) {
                return params["diagnostics"].as_array().cloned().unwrap_or_default();
            }
        }
    }

    /// Every diagnostics publish for `path` that arrives within `wait`.
    async fn publishes_within(&mut self, path: &Path, wait: Duration) -> usize {
        let deadline = tokio::time::Instant::now() + wait;
        let mut count = 0;
        while let Ok(Some(request)) = tokio::time::timeout_at(deadline, self.notifications.recv()).await {
            let params = request.params().cloned().unwrap_or_default();
            if request.method() == "textDocument/publishDiagnostics" && params["uri"] == uri(path) {
                count += 1;
            }
        }
        count
    }
}

fn position(line: u32, character: u32) -> Value {
    json!({ "line": line, "character": character })
}

fn at(path: &Path, line: u32, character: u32) -> Value {
    json!({ "textDocument": { "uri": uri(path) }, "position": position(line, character) })
}

/// Apply the LSP edits in a workspace edit's `changes` to `text`.
fn apply(text: &str, edits: &Value) -> String {
    let mut edits: Vec<&Value> = edits.as_array().map(|it| it.iter().collect()).unwrap_or_default();
    edits.sort_by_key(|edit| {
        std::cmp::Reverse((edit["range"]["start"]["line"].as_u64(), edit["range"]["start"]["character"].as_u64()))
    });
    let mut out = text.to_string();
    for edit in edits {
        let index = LineIndex::new(&out);
        let point = |value: &Value| tower_lsp::lsp_types::Position {
            line: value["line"].as_u64().unwrap() as u32,
            character: value["character"].as_u64().unwrap() as u32,
        };
        let start = u32::from(index.offset(point(&edit["range"]["start"]))) as usize;
        let end = u32::from(index.offset(point(&edit["range"]["end"]))) as usize;
        out.replace_range(start..end, edit["newText"].as_str().unwrap());
    }
    out
}

#[tokio::test]
async fn the_server_advertises_what_the_spec_requires() {
    let root = project(&[("main.pi", "a: 1\n")]);
    let editor = Editor::open(&root).await;
    let capabilities = &editor.capabilities;
    assert_eq!(capabilities["documentHighlightProvider"], true);
    assert_eq!(capabilities["renameProvider"]["prepareProvider"], true);
    let kinds = capabilities["codeActionProvider"]["codeActionKinds"].to_string();
    for kind in ["quickfix", "source.organizeImports", "source.removeUnusedImports"] {
        assert!(kinds.contains(kind), "{kinds}");
    }
    assert!(capabilities["workspace"]["fileOperations"]["willRename"].is_object());
}

#[tokio::test]
async fn preparing_a_rename_on_self_is_refused_with_a_reason() {
    let root = project(&[("main.pi", "anchor A:\n    name: x\n    other: ${self.name}\n")]);
    let main = root.join("main.pi");
    let mut editor = Editor::open(&root).await;
    let refused = editor.request("textDocument/prepareRename", at(&main, 2, 14)).await.expect_err("refused");
    assert!(refused.message.contains("self"), "{}", refused.message);
    // The property after the dot can be renamed.
    let prepared = editor.request("textDocument/prepareRename", at(&main, 2, 19)).await.expect("renameable");
    assert_eq!(prepared["placeholder"], "name");
}

#[tokio::test]
async fn a_rename_that_would_collide_is_refused_before_anything_changes() {
    let root = project(&[("main.pi", "anchor A:\n    x: 1\n\nanchor B:\n    y: 2\n")]);
    let main = root.join("main.pi");
    let mut editor = Editor::open(&root).await;
    let mut params = at(&main, 0, 7);
    params["newName"] = json!("B");
    let refused = editor.request("textDocument/rename", params).await.expect_err("refused");
    assert!(refused.message.contains("already"), "{}", refused.message);
    let mut params = at(&main, 0, 7);
    params["newName"] = json!("not a name");
    let refused = editor.request("textDocument/rename", params).await.expect_err("refused");
    assert!(refused.message.contains("not a name"), "{}", refused.message);
}

#[tokio::test]
async fn definition_follows_a_member_access_to_the_anchor_that_holds_it() {
    let root = project(&[
        ("other.pi", "export anchor Thing:\n    x: 1\n"),
        ("main.pi", "from ./other import Thing\n\nanchor Child:\n    value: {Thing.x}\n"),
    ]);
    let mut editor = Editor::open(&root).await;
    let found = editor
        .request("textDocument/definition", at(&root.join("main.pi"), 3, 18))
        .await
        .expect("answered");
    assert_eq!(found["uri"], uri(&root.join("other.pi")), "{found}");
    assert_eq!(found["range"]["start"], position(1, 4), "{found}");
}

#[tokio::test]
async fn highlights_mark_the_declaration_as_a_write() {
    let root = project(&[("main.pi", "anchor A:\n    x: 1\n    y: {self.x}\n")]);
    let mut editor = Editor::open(&root).await;
    let found = editor
        .request("textDocument/documentHighlight", at(&root.join("main.pi"), 1, 4))
        .await
        .expect("answered");
    let kinds: Vec<(u64, u64)> = found
        .as_array()
        .unwrap()
        .iter()
        .map(|it| (it["range"]["start"]["line"].as_u64().unwrap(), it["kind"].as_u64().unwrap()))
        .collect();
    assert_eq!(kinds, [(1, 3), (2, 2)]);
}

#[tokio::test]
async fn code_actions_organize_imports_on_request_and_never_offer_formatting() {
    let other = "export anchor Thing:\n    x: 1\n\nexport anchor Unused:\n    y: 1\n";
    let source = "from ./other import Unused, Thing\n\na: {Thing.x}\n";
    let root = project(&[("other.pi", other), ("main.pi", source)]);
    let main = root.join("main.pi");
    let mut editor = Editor::open(&root).await;
    let whole = json!({ "start": position(0, 0), "end": position(3, 0) });

    let organize = editor
        .request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": uri(&main) },
                "range": whole,
                "context": { "diagnostics": [], "only": ["source.organizeImports"] },
            }),
        )
        .await
        .expect("answered");
    let titles: Vec<&str> = organize.as_array().unwrap().iter().filter_map(|it| it["title"].as_str()).collect();
    assert_eq!(titles, ["Organize imports"]);
    let edits = &organize[0]["edit"]["changes"][uri(&main)];
    assert_eq!(apply(source, edits), "from ./other import Thing\n\na: {Thing.x}\n");

    let everything = editor
        .request(
            "textDocument/codeAction",
            json!({ "textDocument": { "uri": uri(&main) }, "range": whole, "context": { "diagnostics": [] } }),
        )
        .await
        .expect("answered");
    assert!(!everything.to_string().contains("Format"), "{everything}");
}

#[tokio::test]
async fn unused_imports_are_faded_hints_and_a_problem_is_reported_once() {
    let root = project(&[
        ("other.pi", "export anchor Thing:\n    x: 1\n"),
        ("main.pi", "from ./other import Thing\n\na: {Missing.x}\n"),
    ]);
    let main = root.join("main.pi");
    let mut editor = Editor::open(&root).await;
    let diagnostics = editor.diagnostics(&main).await;
    let messages: Vec<&str> = diagnostics.iter().filter_map(|it| it["message"].as_str()).collect();
    assert_eq!(diagnostics.len(), 2, "{messages:?}");
    assert!(messages.iter().any(|it| it.contains("cannot find `Missing`")), "{messages:?}");
    let unused = diagnostics.iter().find(|it| it["code"] == "unused-import").expect("an unused import");
    assert_eq!(unused["severity"], 4, "a hint");
    assert_eq!(unused["tags"], json!([1]), "marked as unnecessary");
}

#[tokio::test]
async fn a_burst_of_changes_publishes_once() {
    let root = project(&[("main.pi", "a: 1\n")]);
    let main = root.join("main.pi");
    let mut editor = Editor::open(&root).await;
    editor
        .notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri(&main), "languageId": "piton", "version": 1, "text": "a: 1\n" } }),
        )
        .await;
    for version in 2..8 {
        let text = if version % 2 == 0 { "a: {Missing}\n" } else { "a: {Other}\n" };
        editor
            .notify(
                "textDocument/didChange",
                json!({
                    "textDocument": { "uri": uri(&main), "version": version },
                    "contentChanges": [{ "text": text }],
                }),
            )
            .await;
    }
    assert_eq!(editor.publishes_within(&main, Duration::from_millis(800)).await, 1);
}

#[tokio::test]
async fn formatting_touches_only_the_lines_that_change() {
    let source = "anchor A:\n    name: v\n\n\n\nb: 1\n";
    let root = project(&[("main.pi", source)]);
    let main = root.join("main.pi");
    let mut editor = Editor::open(&root).await;
    let edits = editor
        .request(
            "textDocument/formatting",
            json!({ "textDocument": { "uri": uri(&main) }, "options": { "tabSize": 4, "insertSpaces": true } }),
        )
        .await
        .expect("answered");
    assert_eq!(edits.as_array().map(Vec::len), Some(1), "{edits}");
    assert!(edits[0]["range"]["start"]["line"].as_u64().unwrap() >= 2, "{edits}");
    assert_eq!(apply(source, &edits), "anchor A:\n    name: v\n\nb: 1\n");
}

#[tokio::test]
async fn moves_asked_about_back_to_back_come_out_right_in_the_editor() {
    let canvas = "from ./DirectSelectTool import DirectSelectTool\nfrom ./SelectTool import SelectTool\n\n\
                  a: {SelectTool.kind}\nb: {DirectSelectTool.kind}\n";
    let direct = "from ./SelectTool import SelectTool\n\nexport anchor DirectSelectTool:\n    kind: {SelectTool.kind}\n";
    let root = project(&[
        ("piton.config.pi", "use @piton/config\n\nexport piton-config Config:\n    root: ./\n"),
        ("CanvasConcept.pi", canvas),
        ("SelectTool.pi", "export anchor SelectTool:\n    kind: select\n"),
        ("DirectSelectTool.pi", direct),
    ]);
    let mut editor = Editor::open(&root).await;
    let mut texts = std::collections::HashMap::from([
        (uri(&root.join("CanvasConcept.pi")), canvas.to_string()),
        (uri(&root.join("DirectSelectTool.pi")), direct.to_string()),
    ]);
    for (from, to) in [("SelectTool.pi", "tools/SelectTool.pi"), ("DirectSelectTool.pi", "tools/DirectSelectTool.pi")] {
        let edit = editor
            .request(
                "workspace/willRenameFiles",
                json!({ "files": [{ "oldUri": uri(&root.join(from)), "newUri": uri(&root.join(to)) }] }),
            )
            .await
            .expect("answered");
        for (file, edits) in edit["changes"].as_object().into_iter().flatten() {
            let text = texts.get(file).cloned().unwrap_or_default();
            texts.insert(file.clone(), apply(&text, edits));
        }
    }
    let direct = &texts[&uri(&root.join("DirectSelectTool.pi"))];
    assert!(direct.starts_with("from ./SelectTool import SelectTool\n"), "{direct}");
    let canvas = &texts[&uri(&root.join("CanvasConcept.pi"))];
    assert!(canvas.starts_with("from ./tools/DirectSelectTool import DirectSelectTool\nfrom ./tools/SelectTool"), "{canvas}");
}

#[tokio::test]
async fn workspace_symbols_match_in_order_and_list_a_shared_file_once() {
    let root = project(&[
        ("shared/Tool.pi", "export abstract anchor Tool as tool:\n    purpose:: string\n"),
        (
            "origin/piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n\n    sharedRoot: ../shared\n",
        ),
        ("origin/spec/index.pi", "use //Tool\n\nexport tool Hammer:\n    purpose: drive nails\n"),
        (
            "substrate/piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./\n\n    sharedRoot: ../shared\n",
        ),
        ("substrate/index.pi", "use //Tool\n\nexport tool Wrench:\n    purpose: turn bolts\n"),
    ]);
    let mut editor = Editor::open(&root).await;
    let found = editor.request("workspace/symbol", json!({ "query": "tl" })).await.expect("answered");
    let found = found.as_array().cloned().unwrap_or_default();
    let tools: Vec<&Value> = found.iter().filter(|it| it["name"] == "Tool").collect();
    assert_eq!(tools.len(), 1, "{found:?}");
    assert!(tools[0]["containerName"].as_str().is_some_and(|it| it.ends_with("Tool.pi")), "{found:?}");
    assert!(!found.iter().any(|it| it["name"] == "Wrench"), "`tl` is not in `Wrench` in order");
}

#[tokio::test]
async fn a_fixed_problem_is_cleared_in_the_editor() {
    let root = project(&[("main.pi", "a: {Missing}\n")]);
    let main = root.join("main.pi");
    let mut editor = Editor::open(&root).await;
    assert_eq!(editor.diagnostics(&main).await.len(), 1);
    editor
        .notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri(&main), "languageId": "piton", "version": 1, "text": "a: {Missing}\n" } }),
        )
        .await;
    editor
        .notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri(&main), "version": 2 }, "contentChanges": [{ "text": "a: 1\n" }] }),
        )
        .await;
    assert!(editor.diagnostics(&main).await.is_empty(), "the editor is told the problem is gone");
}

#[tokio::test]
async fn pulled_diagnostics_are_the_ones_published() {
    let root = project(&[
        ("other.pi", "export anchor Thing:\n    x: 1\n"),
        ("main.pi", "from ./other import Thing\n\na: 1\n"),
    ]);
    let main = root.join("main.pi");
    let mut editor = Editor::open(&root).await;
    let published = editor.diagnostics(&main).await;
    let pulled = editor
        .request("textDocument/diagnostic", json!({ "textDocument": { "uri": uri(&main) } }))
        .await
        .expect("answered");
    assert_eq!(pulled["items"].as_array().cloned().unwrap_or_default(), published);
    assert_eq!(published.len(), 1);
    assert_eq!(published[0]["code"], "unused-import");
}
