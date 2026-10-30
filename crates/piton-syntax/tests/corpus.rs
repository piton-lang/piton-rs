//! Parses every `.pi` file in the repository.
//!
//! The specification is written in Piton, so the specification itself is the
//! parser's acceptance suite: it must parse without errors and the lossless tree
//! must reproduce every byte.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn piton_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !matches!(name.as_ref(), "target" | ".git" | "node_modules") {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "pi") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn the_specification_parses_cleanly() {
    let root = repo_root();
    let files = piton_files(&root);
    assert!(files.len() > 100, "expected the spec corpus, found {}", files.len());

    let mut failures = Vec::new();
    for path in &files {
        let source = std::fs::read_to_string(path).expect("readable");
        let parse = piton_syntax::parser::parse(&source, path);

        if parse.syntax().text().to_string() != source {
            failures.push(format!("{}: lossless round-trip failed", path.display()));
        }
        for diagnostic in parse.diagnostics.iter().filter(|d| d.is_error()) {
            failures.push(format!(
                "{}: {} [{}]",
                path.strip_prefix(&root).unwrap_or(path).display(),
                diagnostic.message,
                diagnostic.code
            ));
        }
    }
    assert!(failures.is_empty(), "{} problems:\n{}", failures.len(), failures.join("\n"));
}

/// Every name the program knows about is a token of its own in the tree.
///
/// Editors read the tree, not the AST: highlighting, completion and the
/// golden classification all ask what token is at a position. A header or
/// import line whose words are left as prose is a line nothing can colour or
/// complete, and a name whose AST span is not exactly one token is a name
/// rename and hover miss.
#[test]
fn names_in_the_specification_are_tokens() {
    use piton_syntax::ast::{BlockItem, Item};
    use piton_syntax::SyntaxKind;

    let root = repo_root();
    let mut failures = Vec::new();
    for path in piton_files(&root) {
        let source = std::fs::read_to_string(&path).expect("readable");
        let parse = piton_syntax::parser::parse(&source, &path);
        let shown = path.strip_prefix(&root).unwrap_or(&path).display().to_string();
        let tree = parse.syntax();
        let tokens: Vec<_> = tree
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .collect();

        for token in &tokens {
            if token.kind() != SyntaxKind::PROSE {
                continue;
            }
            let structural = token.parent_ancestors().find(|node| {
                matches!(
                    node.kind(),
                    SyntaxKind::USE_DECL
                        | SyntaxKind::FROM_DECL
                        | SyntaxKind::ANCHOR_HEADER
                        | SyntaxKind::TYPE_CONSTRAINT
                        | SyntaxKind::REEXPORT_DECL
                        | SyntaxKind::INTERPOLATION
                )
            });
            if let Some(node) = structural {
                failures.push(format!("{shown}: prose {:?} inside {:?}", token.text(), node.kind()));
            }
        }

        let mut names = Vec::new();
        for item in &parse.file.items {
            match item {
                Item::Anchor(anchor) => {
                    names.push(anchor.name_span);
                    names.extend(anchor.extends.iter().map(|base| base.span));
                    names.extend(anchor.alias.iter().map(|alias| alias.span));
                    let mut blocks = vec![&anchor.body];
                    while let Some(block) = blocks.pop() {
                        for entry in &block.items {
                            if let BlockItem::Property(property) = entry {
                                names.push(property.name_span);
                                blocks.extend(property.value.block.iter());
                            }
                        }
                    }
                }
                Item::Variable(variable) => names.push(variable.name_span),
                Item::From(from) => {
                    for import in &from.items {
                        names.push(import.name_span);
                        names.extend(import.alias.iter().map(|alias| alias.span));
                    }
                }
                _ => {}
            }
        }
        for span in names {
            let exact = tokens.iter().any(|token| {
                token.kind() == SyntaxKind::IDENT
                    && usize::from(token.text_range().start()) == span.start
                    && usize::from(token.text_range().end()) == span.end
            });
            if !exact {
                failures.push(format!("{shown}: {:?} is not one token", &source[span.start..span.end]));
            }
        }
    }
    assert!(failures.is_empty(), "{} problems:\n{}", failures.len(), failures.join("\n"));
}
