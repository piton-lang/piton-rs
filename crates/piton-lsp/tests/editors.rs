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
        let styled = ["style", "foreground_color", "background_color", "font_style", "font_weight"]
            .iter()
            .any(|key| rule.get(key).is_some());
        assert!(styled, "a rule in `semantic_token_rules.json` styles nothing: {rule}");
    }
}
