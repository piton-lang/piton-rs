//! Completion.
//!
//! Every suggestion is decided by *where* the cursor is, worked out from the
//! syntax tree rather than from guessing at the line. Piton has few places a
//! name can appear, so each place offers only what is valid there — and prose,
//! which is most of a Piton file, offers nothing at all.

use std::collections::HashSet;

use piton_core::db::{ModuleCandidate, ModuleOrigin};
use piton_core::resolve::Symbol;
use piton_core::value::{AnchorId, Value};
use piton_core::FileId;
use piton_syntax::ast::{self, AstNode};
use piton_syntax::kind::SyntaxKind::{self, *};
use piton_syntax::kind::{BUILTIN_TYPES, SELF_KEYWORDS};
use piton_syntax::{SyntaxNode, TextRange, TextSize};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Documentation, InsertTextFormat,
    MarkupContent, MarkupKind, TextEdit,
};

use crate::imports;
use crate::index::{Holder, Member, Model, Sym};
use crate::world::View;

/// What the cursor is in the middle of writing.
enum Context {
    /// Prose, or anywhere else nothing can be suggested.
    Nothing,
    /// A `from`/`use` module specifier.
    ModulePath { typed: String, range: TextRange },
    /// The name list of `from PATH import ...`, and the names it already has.
    ImportNames { module: FileId, listed: Vec<String> },
    /// After a complete `from PATH`, where `import` or `export` belongs.
    ImportVerb,
    /// After `::`.
    Type { in_abstract: bool },
    /// After `extends`.
    Base,
    /// Inside `{ ... }` after a `.`.
    Member { path: Vec<String> },
    /// Inside `{ ... }`.
    Expression,
    /// The start of an indented line: a property key.
    PropertyKey,
    /// The start of a top-level line.
    Declaration { after_export: bool, after_abstract: bool },
}

/// Suggest completions at an offset.
pub fn complete(view: &View, file: FileId, offset: TextSize) -> Vec<CompletionItem> {
    let typed = typed_name(view, file, offset);
    suggestions(view, file, offset).into_iter().map(|item| replacing(item, typed)).collect()
}

/// The name being typed before the cursor.
///
/// A name can contain `-`, and editors end a word at one, so an item that left
/// the range to the editor replaced only `comp` of `ui-comp` and wrote
/// `ui-ui-component`.
fn typed_name(view: &View, file: FileId, offset: TextSize) -> tower_lsp::lsp_types::Range {
    let text = view.text(file);
    let cursor = usize::from(offset).min(text.len());
    let start = text[..cursor]
        .char_indices()
        .rev()
        .take_while(|(_, it)| it.is_alphanumeric() || matches!(it, '_' | '-'))
        .last()
        .map_or(cursor, |(at, _)| at);
    view.line_index(file).range(TextRange::new(TextSize::new(start as u32), TextSize::new(cursor as u32)))
}

/// Say exactly what accepting an item replaces, unless it already does.
fn replacing(mut item: CompletionItem, typed: tower_lsp::lsp_types::Range) -> CompletionItem {
    if item.text_edit.is_some() {
        return item;
    }
    let new_text = item.insert_text.take().unwrap_or_else(|| item.label.clone());
    item.filter_text.get_or_insert_with(|| item.label.clone());
    item.text_edit =
        Some(tower_lsp::lsp_types::CompletionTextEdit::Edit(TextEdit { range: typed, new_text }));
    item
}

fn suggestions(view: &View, file: FileId, offset: TextSize) -> Vec<CompletionItem> {
    match context(view, file, offset) {
        Context::Nothing => Vec::new(),
        Context::ModulePath { typed, range } => module_items(view, file, &typed, range),
        Context::ImportNames { module, listed } => export_items(view, module, &listed),
        Context::ImportVerb => vec![
            item("import", CompletionItemKind::KEYWORD, "bring names into this file"),
            item("export", CompletionItemKind::KEYWORD, "import and republish in one line"),
        ],
        Context::Type { in_abstract } => type_items(view, file, in_abstract),
        Context::Base => ranked(anchor_items(view, file), importable(view, file, true)),
        Context::Member { path } => member_items(view, file, offset, &path),
        Context::Expression => ranked(expression_items(view, file, offset), importable(view, file, false)),
        Context::PropertyKey => property_items(view, file, offset),
        Context::Declaration { after_export, after_abstract } => {
            declaration_items(view, file, after_export, after_abstract)
        }
    }
}

/// Names already in scope first, then the ones that would be imported.
fn ranked(mut in_scope: Vec<CompletionItem>, importable: Vec<CompletionItem>) -> Vec<CompletionItem> {
    for item in &mut in_scope {
        item.sort_text.get_or_insert_with(|| format!("0{}", item.label.to_lowercase()));
    }
    in_scope.extend(importable);
    in_scope
}

// ---- working out where the cursor is ---------------------------------------

fn context(view: &View, file: FileId, offset: TextSize) -> Context {
    let root = view.compilation.analysis.db.file(file).parse.syntax();
    let text = view.text(file);
    let cursor = usize::from(offset).min(text.len());
    let line_start = text[..cursor].rfind('\n').map_or(0, |it| it + 1);
    let prefix = &text[line_start..cursor];

    let element = root.covering_element(TextRange::empty(offset));
    let node = match &element {
        piton_syntax::NodeOrToken::Node(node) => node.clone(),
        piton_syntax::NodeOrToken::Token(token) => match token.parent() {
            Some(parent) => parent,
            None => root.clone(),
        },
    };

    // What a fence or an escape group holds is kept exactly as written and
    // never read as Piton, however much of it looks like an import or an
    // expression.
    if node.ancestors().any(|it| it.kind() == CODE_BLOCK) {
        return Context::Nothing;
    }
    if element.kind() == ESCAPE_GROUP && offset > element.text_range().start() {
        return Context::Nothing;
    }

    // A half-typed `from`/`use` line does not parse, which is exactly when
    // completion runs, so it is read from the line rather than from the tree.
    if let Some(context) = import_line_context(view, file, offset, prefix) {
        return context;
    }

    for ancestor in node.ancestors() {
        match ancestor.kind() {
            // The declaration owns its newline, so the start of the next line
            // is the end of its range without being part of it.
            IMPORT_DECL | REEXPORT_DECL | USE_DECL
                if !(offset == ancestor.text_range().end() && text[..cursor].ends_with('\n')) =>
            {
                return import_context(view, file, &ancestor, offset, prefix)
            }
            IMPORT_DECL | REEXPORT_DECL | USE_DECL => break,
            TYPE_ANNOTATION | TYPE_REF | TYPE_LIST | TYPE_EXTENDS => {
                return Context::Type { in_abstract: in_abstract_anchor(&ancestor) }
            }
            EXTENDS_CLAUSE => return Context::Base,
            AS_CLAUSE => return Context::Nothing,
            BRACE_EXPR | INTERPOLATION => return expression_context(prefix),
            _ => {}
        }
    }

    // Outside a structured region: `{` still opens an expression while typing,
    // because an unterminated brace has no node to sit in yet.
    if prefix.matches('{').count() > prefix.matches('}').count() {
        return expression_context(prefix);
    }

    line_start_context(view, file, offset, prefix)
}

/// Read a `from`/`use` line straight from the text.
///
/// Returns `None` when the line is not one, so the tree still decides.
fn import_line_context(
    view: &View,
    file: FileId,
    offset: TextSize,
    prefix: &str,
) -> Option<Context> {
    let indent = prefix.len() - prefix.trim_start().len();
    let body = prefix.trim_start();
    let specifier_at = |consumed: usize, rest: &str| -> TextSize {
        let leading = rest.len() - rest.trim_start().len();
        offset - TextSize::new((prefix.len() - indent - consumed - leading) as u32)
    };

    for keyword in ["from", "use"] {
        let Some(rest) = body.strip_prefix(keyword) else { continue };
        if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
            continue;
        }
        let start = specifier_at(keyword.len(), rest);
        let typed = rest.trim_start();

        // Still inside the specifier: no whitespace has ended it yet.
        let Some(end) = typed.find([' ', '\t']) else {
            return Some(Context::ModulePath {
                typed: typed.to_string(),
                range: TextRange::new(start, offset),
            });
        };
        if keyword == "use" {
            return Some(Context::Nothing);
        }

        let specifier = &typed[..end];
        let tail = typed[end..].trim_start();
        for verb in ["import", "export"] {
            let Some(names) = tail.strip_prefix(verb) else { continue };
            if !names.is_empty() && !names.starts_with([' ', '\t']) {
                continue;
            }
            let module = view.compilation.analysis.db.lookup_module(file, specifier)?;
            // The name after the last comma is the one being typed, so it does
            // not rule anything out.
            let mut listed: Vec<String> = names
                .split(',')
                .filter_map(|it| it.split_whitespace().next().map(str::to_string))
                .collect();
            if !names.trim_end().ends_with(',') {
                listed.pop();
            }
            return Some(Context::ImportNames { module, listed });
        }
        return Some(if tail.is_empty() { Context::ImportVerb } else { Context::Nothing });
    }
    None
}

/// `from PATH import a, b` — the path, then the names it exports.
fn import_context(
    view: &View,
    file: FileId,
    declaration: &SyntaxNode,
    offset: TextSize,
    prefix: &str,
) -> Context {
    let token_of = |kind: SyntaxKind| {
        declaration
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == kind)
    };
    let verb = token_of(IMPORT_KW).or_else(|| token_of(EXPORT_KW));
    if let Some(verb) = &verb {
        if offset > verb.text_range().end() {
            let module = token_of(PATH)
                .and_then(|path| view.compilation.analysis.module(file, path.text()));
            let listed = declaration
                .children()
                .find_map(ast::ImportList::cast)
                .map(|list| {
                    list.items()
                        .filter(|item| !item.syntax().text_range().contains_inclusive(offset))
                        .filter_map(|item| item.name())
                        .collect()
                })
                .unwrap_or_default();
            return match module {
                Some(module) => Context::ImportNames { module, listed },
                None => Context::Nothing,
            };
        }
    }
    match token_of(PATH) {
        // Re-completing an existing path replaces the whole token.
        Some(path) if path.text_range().contains_inclusive(offset) => Context::ModulePath {
            typed: path.text()[..usize::from(offset - path.text_range().start())].to_string(),
            range: TextRange::new(path.text_range().start(), offset),
        },
        Some(_) => Context::Nothing,
        None => {
            // No path yet: the specifier starts at the cursor.
            let typed = prefix.rsplit([' ', '\t']).next().unwrap_or_default().to_string();
            let start = offset - TextSize::new(typed.len() as u32);
            Context::ModulePath { typed, range: TextRange::new(start, offset) }
        }
    }
}

/// Inside `{ }`: a trailing `.` asks for members of what precedes it.
fn expression_context(prefix: &str) -> Context {
    let expression = match prefix.rfind('{') {
        Some(at) => &prefix[at + 1..],
        None => prefix,
    };
    let reference: String = expression
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if !reference.contains('.') {
        return Context::Expression;
    }
    // The final segment is what is being typed, so it is not part of the path.
    let mut path: Vec<String> = reference.split('.').map(str::to_string).collect();
    path.pop();
    Context::Member { path }
}

/// A line that has only whitespace, or a partial word, before the cursor.
fn line_start_context(
    view: &View,
    file: FileId,
    offset: TextSize,
    prefix: &str,
) -> Context {
    let head = prefix.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '-');
    let indented = prefix.starts_with([' ', '\t']);
    match head.trim() {
        "" => {
            if indented {
                // Inside a run of prose a word is a word: writing `and:` there
                // makes a sentence, not a property, so offering property names
                // would suggest something the compiler will not read as one.
                // Only a blank line reopens the position.
                let text = view.text(file);
                let cursor = usize::from(offset).min(text.len());
                let line_start = text[..cursor].rfind('\n').map_or(0, |it| it + 1);
                if piton_syntax::in_prose_run(text, line_start) {
                    return Context::Nothing;
                }
                Context::PropertyKey
            } else {
                Context::Declaration { after_export: false, after_abstract: false }
            }
        }
        "export" if !indented => {
            Context::Declaration { after_export: true, after_abstract: false }
        }
        "abstract" | "export abstract" if !indented => {
            Context::Declaration { after_export: false, after_abstract: true }
        }
        _ => Context::Nothing,
    }
}

fn in_abstract_anchor(node: &SyntaxNode) -> bool {
    node.ancestors()
        .find(|it| it.kind() == ANCHOR_DECL)
        .and_then(piton_syntax::ast::AnchorDecl::cast)
        .is_some_and(|anchor| anchor.abstract_token().is_some())
}

// ---- the suggestions themselves ---------------------------------------------

fn item(label: &str, kind: CompletionItemKind, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: Some(detail.to_string()),
        ..CompletionItem::default()
    }
}

fn snippet(label: &str, insert: &str, detail: &str, docs: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(detail.to_string()),
        insert_text: Some(insert.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: docs.to_string(),
        })),
        ..CompletionItem::default()
    }
}

/// Files and directories that could finish a `from`/`use` specifier.
fn module_items(
    view: &View,
    file: FileId,
    typed: &str,
    range: TextRange,
) -> Vec<CompletionItem> {
    let index = view.line_index(file);
    view
        .compilation
        .analysis
        .db
        .complete_specifier(file, typed)
        .into_iter()
        .map(|candidate| {
            // Only the leaf is replaced once a directory has been typed, so the
            // client filters on what the reader is actually looking at.
            let start = range.end() - TextSize::new((typed.len() - candidate.replace_from) as u32);
            module_item(candidate, index.range(TextRange::new(start, range.end())))
        })
        .collect()
}

fn module_item(
    candidate: ModuleCandidate,
    replace: tower_lsp::lsp_types::Range,
) -> CompletionItem {
    let (kind, what) = match (candidate.directory, candidate.importable) {
        (true, true) => (CompletionItemKind::MODULE, "module directory"),
        (true, false) => (CompletionItemKind::FOLDER, "directory"),
        (false, _) => (CompletionItemKind::FILE, "module"),
    };
    let where_from = match candidate.origin {
        ModuleOrigin::Relative => "beside this file",
        ModuleOrigin::Root => "from the project root",
        ModuleOrigin::Shared => "from the shared root",
        ModuleOrigin::Builtin => "built in",
    };
    // A trailing slash says "there is more to type" at a glance.
    let label =
        if candidate.directory && !candidate.importable {
            format!("{}/", candidate.name)
        } else {
            candidate.name.clone()
        };
    CompletionItem {
        label,
        kind: Some(kind),
        // The full specifier, so it is obvious what will be written.
        detail: Some(format!("{}  —  {what} {where_from}", candidate.specifier)),
        filter_text: Some(candidate.name.clone()),
        // Relative, then root, then builtin; importable before directories.
        sort_text: Some(format!(
            "{}{}{}",
            candidate.origin as u8,
            u8::from(!candidate.importable),
            candidate.name.to_lowercase()
        )),
        text_edit: Some(tower_lsp::lsp_types::CompletionTextEdit::Edit(TextEdit {
            range: replace,
            new_text: candidate.insert,
        })),
        ..CompletionItem::default()
    }
}

/// What a module actually exports and the import does not name yet, so an
/// import list cannot be wrong.
fn export_items(view: &View, module: FileId, listed: &[String]) -> Vec<CompletionItem> {
    let analysis = &view.compilation.analysis;
    analysis
        .scope(module)
        .exports
        .iter()
        .filter(|(name, _)| !listed.contains(name))
        .map(|(name, symbol)| match symbol {
            Symbol::Anchor(id) => {
                let definition = analysis.anchor_def(*id);
                item(
                    name,
                    CompletionItemKind::CLASS,
                    if definition.is_abstract { "abstract anchor" } else { "anchor" },
                )
            }
            Symbol::Var { .. } => item(name, CompletionItemKind::CONSTANT, "variable"),
        })
        .collect()
}

fn type_items(view: &View, file: FileId, in_abstract: bool) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = BUILTIN_TYPES
        .iter()
        .map(|name| item(name, CompletionItemKind::KEYWORD, "built-in type"))
        .collect();
    // `extends` as a constraint only means something inside an abstract anchor.
    if in_abstract {
        items.push(item(
            "extends",
            CompletionItemKind::KEYWORD,
            "any anchor whose chain includes this one",
        ));
    }
    items.extend(anchor_items(view, file));
    ranked(items, importable(view, file, true))
}

fn anchor_items(view: &View, file: FileId) -> Vec<CompletionItem> {
    let analysis = &view.compilation.analysis;
    analysis
        .scope(file)
        .names
        .iter()
        .filter_map(|(name, symbol)| match symbol {
            Symbol::Anchor(id) => Some(item(
                name,
                CompletionItemKind::CLASS,
                if analysis.anchor_def(*id).is_abstract { "abstract anchor" } else { "anchor" },
            )),
            Symbol::Var { .. } => None,
        })
        .collect()
}

/// Names visible in an expression, plus the self-reference keywords.
fn expression_items(view: &View, file: FileId, offset: TextSize) -> Vec<CompletionItem> {
    let analysis = &view.compilation.analysis;
    let mut items: Vec<CompletionItem> = analysis
        .scope(file)
        .names
        .iter()
        .map(|(name, symbol)| match symbol {
            Symbol::Anchor(id) => item(
                name,
                CompletionItemKind::CLASS,
                if analysis.anchor_def(*id).is_abstract { "abstract anchor" } else { "anchor" },
            ),
            Symbol::Var { .. } => item(name, CompletionItemKind::VARIABLE, "variable"),
        })
        .collect();
    if enclosing(view, file, offset).is_some() {
        for keyword in SELF_KEYWORDS {
            items.push(item(keyword, CompletionItemKind::KEYWORD, "self reference"));
        }
    }
    items
}

/// Every name another module exports that the file cannot see yet, each with
/// the edit that imports it.
fn importable(view: &View, file: FileId, anchors_only: bool) -> Vec<CompletionItem> {
    let model = view.model();
    let analysis = model.analysis();
    let scope = analysis.scope(file);
    let mut seen: HashSet<(String, Sym)> = HashSet::new();
    let mut items = Vec::new();
    for module in analysis.db.files() {
        if module.id == file {
            continue;
        }
        for (name, symbol) in &analysis.scope(module.id).exports {
            if scope.names.contains_key(name) || (anchors_only && !matches!(symbol, Symbol::Anchor(_))) {
                continue;
            }
            let Some(sym) = model.origin(module.id, name) else { continue };
            if !seen.insert((name.clone(), sym.clone())) {
                continue;
            }
            let Some(fix) = imports::import_fix(view, &model, file, name, &sym) else { continue };
            let (kind, what) = match model.anchor_of(&sym) {
                Some(id) if analysis.anchor_def(id).is_abstract => (CompletionItemKind::CLASS, "abstract anchor"),
                Some(_) => (CompletionItemKind::CLASS, "anchor"),
                None => (CompletionItemKind::VARIABLE, "variable"),
            };
            items.push(CompletionItem {
                label: name.clone(),
                kind: Some(kind),
                detail: Some(format!("{what}, imported from {}", fix.specifier)),
                label_details: Some(CompletionItemLabelDetails {
                    detail: None,
                    description: Some(fix.specifier.clone()),
                }),
                sort_text: Some(format!("1{}", name.to_lowercase())),
                additional_text_edits: Some(vec![fix.edit]),
                ..CompletionItem::default()
            });
        }
    }
    items
}

/// The members of whatever `path` holds, for completion after a `.`.
///
/// The holder is worked out the way navigation works it out, so an abstract
/// anchor offers the properties it declares even though it has no compiled
/// value. Only a computed value, which has no written shape, falls back to the
/// members of what it compiled to.
fn member_items(
    view: &View,
    file: FileId,
    offset: TextSize,
    path: &[String],
) -> Vec<CompletionItem> {
    let anchor = enclosing(view, file, offset);
    let model = view.model();
    if let Some(holder) = model.path_holder(file, anchor, path) {
        return model.members(&holder).into_iter().map(|member| member_item(&model, &holder, &member)).collect();
    }
    let Some((root, rest)) = path.split_first() else { return Vec::new() };
    if SELF_KEYWORDS.contains(&root.as_str()) {
        return Vec::new();
    }
    let Some(mut value) = resolve_value(view, file, root) else { return Vec::new() };
    for segment in rest {
        let Some(next) = value.field(segment) else { return Vec::new() };
        value = next.clone();
    }
    members_of(&value)
}

fn member_item(model: &Model, holder: &Holder, member: &Member) -> CompletionItem {
    let name = &member.property.name;
    let slot = match holder {
        Holder::Anchor(id) => model.slot(*id, name),
        Holder::Super(id) => model.base_slot(*id, name),
        Holder::Dictionary { .. } => None,
    };
    let constraints = match slot {
        Some(slot) => slot.constraints.map(|(_, it)| it.to_vec()).unwrap_or_default(),
        None => member.property.constraints.clone(),
    };
    let detail = match constraints.first() {
        Some(constraint) => constraint.render(),
        None if matches!(holder, Holder::Dictionary { .. }) => "key".to_string(),
        None => "property".to_string(),
    };
    item(name, CompletionItemKind::PROPERTY, &detail)
}

fn members_of(value: &Value) -> Vec<CompletionItem> {
    let entries: Vec<(&String, &Value)> = match value {
        Value::Dict(dict) => dict.iter().collect(),
        Value::Anchor(anchor) => anchor.props.iter().collect(),
        Value::List(list) if list.implicit => list
            .items
            .iter()
            .filter_map(|item| match item {
                Value::Dict(dict) => Some(dict.iter()),
                _ => None,
            })
            .flatten()
            .collect(),
        _ => return Vec::new(),
    };
    entries
        .into_iter()
        .map(|(name, value)| item(name, CompletionItemKind::PROPERTY, value.type_name()))
        .collect()
}

fn resolve_value(view: &View, file: FileId, name: &str) -> Option<Value> {
    match view.compilation.analysis.scope(file).names.get(name)? {
        Symbol::Anchor(id) => {
            view.compilation.anchor(*id).map(|anchor| Value::Anchor(anchor.clone()))
        }
        Symbol::Var { file, index } => view.compilation.vars.get(&(*file, *index)).cloned(),
    }
}

/// Inside an anchor body: the properties it inherits but has not written yet.
fn property_items(view: &View, file: FileId, offset: TextSize) -> Vec<CompletionItem> {
    let Some(anchor) = enclosing(view, file, offset) else { return Vec::new() };
    let model = view.model();
    let written: Vec<String> = model.own(anchor).iter().map(|property| property.name.clone()).collect();
    model
        .ordered_properties(anchor)
        .into_iter()
        .filter(|name| !written.contains(name))
        .map(|name| {
            let detail = model
                .slot(anchor, &name)
                .and_then(|slot| slot.constraints)
                .and_then(|(_, constraints)| constraints.first())
                .map(|constraint| constraint.render())
                .unwrap_or_else(|| "inherited property".to_string());
            let mut completion = item(&name, CompletionItemKind::PROPERTY, &detail);
            completion.insert_text = Some(format!("{name}: "));
            completion
        })
        .collect()
}

/// The start of a top-level line: only what can begin a declaration.
fn declaration_items(
    view: &View,
    file: FileId,
    after_export: bool,
    after_abstract: bool,
) -> Vec<CompletionItem> {
    let analysis = &view.compilation.analysis;
    let mut items = Vec::new();

    if after_abstract {
        items.push(item("anchor", CompletionItemKind::KEYWORD, "declare an abstract anchor"));
        return items;
    }

    items.push(snippet(
        "anchor",
        "anchor ${1:Name}:\n    $0",
        "declare an anchor",
        "A named structural declaration.",
    ));
    items.push(snippet(
        "abstract anchor",
        "abstract anchor ${1:Name} as ${2:keyword}:\n    ${3:property}:: string\n",
        "declare an abstract anchor and its keyword",
        "Abstract anchors describe a shape; exporting one `as` a keyword is the \
         idiomatic way to make it implementable exactly once.",
    ));

    for (keyword, id) in &analysis.scope(file).keywords {
        items.push(keyword_item(view, keyword, *id, None));
    }
    items.extend(importable_keywords(view, file));

    if after_export {
        // `export Name` republishes something already in scope.
        items.extend(analysis.scope(file).names.keys().map(|name| {
            item(name, CompletionItemKind::VARIABLE, "re-export this name")
        }));
        return items;
    }

    items.push(item("export", CompletionItemKind::KEYWORD, "publish a declaration"));
    items.push(snippet(
        "from … import",
        "from ${1:./module} import ${2:Name}",
        "import names from a module",
        "Only exported names can be imported. `use` brings in keywords instead.",
    ));
    items.push(snippet(
        "from … export",
        "from ${1:./module} export *",
        "re-export a module",
        "The concise way to write an `index.pi`.",
    ));
    items.push(snippet(
        "use",
        "use ${1:./module}",
        "bring in keywords",
        "`use` imports only user-defined keywords.",
    ));
    items
}

/// A keyword, as the start of a declaration written with it.
fn keyword_item(view: &View, keyword: &str, id: AnchorId, from: Option<String>) -> CompletionItem {
    let definition = view.compilation.analysis.anchor_def(id);
    let detail = match &from {
        Some(specifier) => format!("shorthand for `extends {}`, brought in by `use {specifier}`", definition.name),
        None => format!("shorthand for `extends {}`", definition.name),
    };
    let mut completion = item(keyword, CompletionItemKind::FUNCTION, &detail);
    completion.insert_text = Some(format!("{keyword} ${{1:Name}}:\n    $0"));
    completion.insert_text_format = Some(InsertTextFormat::SNIPPET);
    if let Some(doc) = &definition.doc {
        completion.documentation = Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: doc.clone(),
        }));
    }
    completion
}

/// Keywords another module exports that the file has not brought in, each
/// adding the `use` line that brings it in.
fn importable_keywords(view: &View, file: FileId) -> Vec<CompletionItem> {
    let model = view.model();
    let analysis = model.analysis();
    let visible = &analysis.scope(file).keywords;
    let mut offered: Vec<AnchorId> = Vec::new();
    let mut items = Vec::new();
    for module in analysis.db.files() {
        if module.id == file {
            continue;
        }
        for symbol in analysis.scope(module.id).exports.values() {
            let Symbol::Anchor(id) = symbol else { continue };
            let Some(keyword) = &analysis.anchor_def(*id).keyword else { continue };
            if visible.contains_key(&keyword.value) || offered.contains(id) {
                continue;
            }
            offered.push(*id);
            let Some(specifier) = imports::best_specifier(&model, file, Some(model.file_of(*id)), |module| {
                analysis.scope(module).exports.values().any(|it| *it == Symbol::Anchor(*id))
            }) else {
                continue;
            };
            let mut completion = keyword_item(view, &keyword.value, *id, Some(specifier.clone()));
            completion.label_details =
                Some(CompletionItemLabelDetails { detail: None, description: Some(specifier.clone()) });
            completion.sort_text = Some(format!("1{}", keyword.value));
            completion.additional_text_edits = Some(vec![imports::use_edit(view, file, &specifier)]);
            items.push(completion);
        }
    }
    items
}

/// The anchor whose body the cursor is in.
///
/// The tree answers this once the body has content. On the first line of an
/// empty body there is no block yet — the indentation is still just an empty
/// line — so the anchor is found by looking back instead.
fn enclosing(view: &View, file: FileId, offset: TextSize) -> Option<AnchorId> {
    let analysis = &view.compilation.analysis;
    let root = analysis.db.file(file).parse.syntax();
    let node = match root.covering_element(TextRange::empty(offset)) {
        piton_syntax::NodeOrToken::Node(node) => node,
        piton_syntax::NodeOrToken::Token(token) => token.parent()?,
    };
    if let Some(declaration) = node.ancestors().find(|it| it.kind() == ANCHOR_DECL) {
        let range = declaration.text_range();
        let hir = &analysis.db.file(file).hir;
        if let Some(index) = hir.anchors.iter().position(|anchor| anchor.range == range) {
            return analysis.anchor_id(file, index);
        }
    }

    let text = view.text(file);
    let cursor = usize::from(offset).min(text.len());
    let (index, _) = analysis.db.file(file).hir.anchors.iter().enumerate().rfind(
        |(_, anchor)| {
            // Only whitespace may separate the declaration from the cursor;
            // anything else means the body has been left behind.
            usize::from(anchor.range.end()) <= cursor
                && text[usize::from(anchor.range.end())..cursor].trim().is_empty()
        },
    )?;
    analysis.anchor_id(file, index)
}
