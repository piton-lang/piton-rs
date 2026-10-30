//! Checks the editor files that describe what the server sends.
//!
//! An editor that styles semantic tokens has to name the server's token types,
//! and nothing connects the two but a string. Adding a type to the legend and
//! forgetting to style it is invisible: the token arrives, the editor has no
//! rule for it, and the text keeps whatever colour the grammar gave it.

use std::path::{Path, PathBuf};

use piton_lsp::{TOKEN_MODIFIERS, TOKEN_TYPES};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn semantic_token_rules() -> Vec<serde_json::Value> {
    let path = repo_root().join("editors/zed/languages/piton/semantic_token_rules.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_str::<Vec<serde_json::Value>>(&text)
        .unwrap_or_else(|error| panic!("{} is not a list of rules: {error}", path.display()))
}

/// Zed has to have a rule for every token type the server can send.
#[test]
fn zed_styles_every_token_type_the_server_sends() {
    let rules = semantic_token_rules();
    let styled: Vec<&str> = rules
        .iter()
        .filter_map(|rule| rule.get("token_type")?.as_str())
        .collect();

    let unstyled: Vec<&str> = TOKEN_TYPES
        .iter()
        .map(|kind| kind.as_str())
        .filter(|kind| !styled.contains(kind))
        .collect();

    assert!(
        unstyled.is_empty(),
        "`editors/zed/languages/piton/semantic_token_rules.json` has no rule \
         for {}, which the server's legend includes",
        unstyled.join(", ")
    );
}

/// And no rule may name a type or a modifier the server never sends.
#[test]
fn zed_styles_nothing_the_server_does_not_send() {
    let types: Vec<&str> = TOKEN_TYPES.iter().map(|kind| kind.as_str()).collect();
    let modifiers: Vec<&str> = TOKEN_MODIFIERS
        .iter()
        .map(|modifier| modifier.as_str())
        .collect();

    for rule in semantic_token_rules() {
        if let Some(kind) = rule.get("token_type").and_then(|kind| kind.as_str()) {
            assert!(
                types.contains(&kind),
                "a rule styles the `{kind}` token type, which is not in the \
                 server's legend"
            );
        }
        let Some(named) = rule.get("token_modifiers").and_then(|list| list.as_array()) else {
            continue;
        };
        for modifier in named.iter().filter_map(|modifier| modifier.as_str()) {
            assert!(
                modifiers.contains(&modifier),
                "a rule styles the `{modifier}` modifier, which is not in the \
                 server's legend"
            );
        }
    }
}

/// Every rule has to say what it does, or it is dead weight in the file.
#[test]
fn every_rule_carries_a_style() {
    for rule in semantic_token_rules() {
        let styled = [
            "style",
            "foreground_color",
            "background_color",
            "font_style",
            "font_weight",
        ]
        .iter()
        .any(|key| rule.get(key).is_some());
        assert!(
            styled,
            "a rule in `semantic_token_rules.json` styles nothing: {rule}"
        );
    }
}

/// The VS Code extension must not register a command the server lists as its
/// own. The language client registers every command in the server's
/// `executeCommandProvider`, VS Code refuses a second registration of the same
/// name, and the client then fails to start -- so the whole extension stops
/// working over one duplicated string.
#[test]
fn the_vscode_extension_does_not_register_the_servers_commands() {
    let path = repo_root().join("editors/vscode/client.js");
    let client = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let registered: Vec<&str> = client
        .split("registerCommand(\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect();
    assert!(!registered.is_empty(), "found no registerCommand calls in client.js");
    for command in [
        piton_lsp::features::SOURCE_TO_OUTPUT,
        piton_lsp::features::SHOW_LOCATION,
    ] {
        assert!(
            !registered.contains(&command),
            "client.js registers `{command}`, which the server already provides: {registered:?}"
        );
    }
}

/// The TextMate name `editors/fixtures/scopes.toml` gives a highlighting class.
///
/// The table is small and flat (`[Class]` then `tm = "..."`), and this crate
/// has no TOML parser, so it is read by line.
fn textmate_scope(class: &str) -> String {
    let path = repo_root().join("editors/fixtures/scopes.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let mut current = "";
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            current = name;
        } else if current == class {
            if let Some(value) = line.strip_prefix("tm = ") {
                return value.trim_matches('"').to_string();
            }
        }
    }
    panic!("scopes.toml has no TextMate name for `{class}`");
}

/// VS Code paints a semantic token with the TextMate scope the extension maps
/// it to, over whatever the grammar painted. When the two are the same scope,
/// a token arriving changes nothing on screen unless resolution has something
/// to add (a lowercase name that is an anchor, a key that is a variable, a
/// bundled anchor). When they differ, every name flickers to a new colour the
/// moment the server answers. So each type the server sends maps to exactly
/// the scope the grammar uses for the same class, and the custom modifiers are
/// declared so VS Code accepts them.
#[test]
fn vscode_semantic_scopes_match_the_grammar() {
    let path = repo_root().join("editors/vscode/package.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let manifest: serde_json::Value =
        serde_json::from_str(&text).expect("package.json is JSON");
    let contributes = &manifest["contributes"];

    let scopes = contributes["semanticTokenScopes"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["language"] == "piton")
        })
        .map(|entry| &entry["scopes"])
        .expect("package.json maps Piton's semantic tokens in semanticTokenScopes");
    let mapped = |selector: &str| -> Vec<String> {
        scopes[selector]
            .as_array()
            .unwrap_or_else(|| panic!("semanticTokenScopes has no `{selector}` entry"))
            .iter()
            .filter_map(|scope| scope.as_str().map(str::to_string))
            .collect()
    };

    for kind in TOKEN_TYPES {
        assert!(
            scopes.get(kind.as_str()).is_some(),
            "semanticTokenScopes has no entry for `{}`, which the server sends",
            kind.as_str()
        );
    }

    for (selector, class) in [
        ("class", "TypeRef"),
        ("class.declaration", "TypeDecl"),
        ("property", "Key"),
        ("variable", "Variable"),
        ("keyword", "Keyword"),
        ("namespace", "ModulePath"),
    ] {
        assert_eq!(
            mapped(selector),
            vec![textmate_scope(class)],
            "`{selector}` has to map to the grammar's scope for {class}, or the \
             colour changes when the token arrives"
        );
    }

    let declared: Vec<&str> = contributes["semanticTokenModifiers"]
        .as_array()
        .map(|list| list.iter().filter_map(|entry| entry["id"].as_str()).collect())
        .unwrap_or_default();
    // The first three are LSP's standard modifiers, which VS Code knows.
    let standard = ["declaration", "abstract", "defaultLibrary"];
    for modifier in TOKEN_MODIFIERS {
        let name = modifier.as_str();
        assert!(
            standard.contains(&name) || declared.contains(&name),
            "the server sends the `{name}` modifier, which package.json does not \
             declare in semanticTokenModifiers"
        );
    }
}
