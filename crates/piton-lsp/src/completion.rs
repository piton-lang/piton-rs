//! Completion.
//!
//! Every suggestion is decided by *where* the cursor is, worked out from the
//! syntax tree rather than from guessing at the line. Piton has few places a
//! name can appear, so each place offers only what is valid there — and prose,
//! which is most of a Piton file, offers nothing at all.

use piton_core::db::{ModuleCandidate, ModuleOrigin};
use piton_core::resolve::Symbol;
use piton_core::value::Value;
use piton_core::FileId;
use piton_syntax::ast::AstNode;
use piton_syntax::kind::SyntaxKind::{self, *};
use piton_syntax::kind::{BUILTIN_TYPES, SELF_KEYWORDS};
use piton_syntax::{SyntaxNode, TextRange, TextSize};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
    TextEdit,
};

use crate::navigation::{enclosing_anchor, visible_properties};
use crate::world::Snapshot;

/// What the cursor is in the middle of writing.
enum Context {
    /// Prose, or anywhere else nothing can be suggested.
    Nothing,
    /// A `from`/`use` module specifier.
    ModulePath { typed: String, range: TextRange },
    /// The name list of `from PATH import ...`.
    ImportNames { module: FileId },
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
pub fn complete(snapshot: &Snapshot, file: FileId, offset: TextSize) -> Vec<CompletionItem> {
    match context(snapshot, file, offset) {
        Context::Nothing => Vec::new(),
        Context::ModulePath { typed, range } => module_items(snapshot, file, &typed, range),
        Context::ImportNames { module } => export_items(snapshot, module),
        Context::ImportVerb => vec![
            item("import", CompletionItemKind::KEYWORD, "bring names into this file"),
            item("export", CompletionItemKind::KEYWORD, "import and republish in one line"),
        ],
        Context::Type { in_abstract } => type_items(snapshot, file, in_abstract),
        Context::Base => anchor_items(snapshot, file),
        Context::Member { path } => member_items(snapshot, file, offset, &path),
        Context::Expression => expression_items(snapshot, file, offset),
        Context::PropertyKey => property_items(snapshot, file, offset),
        Context::Declaration { after_export, after_abstract } => {
            declaration_items(snapshot, file, after_export, after_abstract)
        }
    }
}

// ---- working out where the cursor is ---------------------------------------

fn context(snapshot: &Snapshot, file: FileId, offset: TextSize) -> Context {
    let root = snapshot.compilation.analysis.db.file(file).parse.syntax();
    let text = snapshot.text(file);
    let cursor = usize::from(offset).min(text.len());
    let line_start = text[..cursor].rfind('\n').map_or(0, |it| it + 1);
    let prefix = &text[line_start..cursor];

    // A half-typed `from`/`use` line does not parse, which is exactly when
    // completion runs, so it is read from the line rather than from the tree.
    if let Some(context) = import_line_context(snapshot, file, offset, prefix) {
        return context;
    }

    let node = match root.covering_element(TextRange::empty(offset)) {
        piton_syntax::NodeOrToken::Node(node) => node,
        piton_syntax::NodeOrToken::Token(token) => match token.parent() {
            Some(parent) => parent,
            None => root.clone(),
        },
    };

    for ancestor in node.ancestors() {
        match ancestor.kind() {
            IMPORT_DECL | REEXPORT_DECL | USE_DECL => {
                return import_context(snapshot, file, &ancestor, offset, prefix)
            }
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

    line_start_context(snapshot, file, offset, prefix)
}

/// Read a `from`/`use` line straight from the text.
///
/// Returns `None` when the line is not one, so the tree still decides.
fn import_line_context(
    snapshot: &Snapshot,
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
            let module = snapshot.compilation.analysis.db.lookup_module(file, specifier)?;
            return Some(Context::ImportNames { module });
        }
        return Some(if tail.is_empty() { Context::ImportVerb } else { Context::Nothing });
    }
    None
}

/// `from PATH import a, b` — the path, then the names it exports.
fn import_context(
    snapshot: &Snapshot,
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
                .and_then(|path| snapshot.compilation.analysis.module(file, path.text()));
            return match module {
                Some(module) => Context::ImportNames { module },
                None => Context::Nothing,
            };
        }
    }
    match token_of(PATH) {
        // Re-completing an existing path replaces the whole token.
        Some(path) if offset >= path.text_range().start() => Context::ModulePath {
            typed: path.text()[..usize::from(offset - path.text_range().start())].to_string(),
            range: TextRange::new(path.text_range().start(), offset),
        },
        _ => {
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
    snapshot: &Snapshot,
    file: FileId,
    offset: TextSize,
    prefix: &str,
) -> Context {
    let head = prefix.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '-');
    let indented = prefix.starts_with([' ', '\t']);
    match head.trim() {
        "" => {
            if indented {
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
        _ => {
            let _ = (snapshot, file, offset);
            Context::Nothing
        }
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
    snapshot: &Snapshot,
    file: FileId,
    typed: &str,
    range: TextRange,
) -> Vec<CompletionItem> {
    let index = snapshot.line_index(file);
    snapshot
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

/// What a module actually exports, so an import list cannot be wrong.
fn export_items(snapshot: &Snapshot, module: FileId) -> Vec<CompletionItem> {
    let analysis = &snapshot.compilation.analysis;
    analysis
        .scope(module)
        .exports
        .iter()
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

fn type_items(snapshot: &Snapshot, file: FileId, in_abstract: bool) -> Vec<CompletionItem> {
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
    items.extend(anchor_items(snapshot, file));
    items
}

fn anchor_items(snapshot: &Snapshot, file: FileId) -> Vec<CompletionItem> {
    let analysis = &snapshot.compilation.analysis;
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
fn expression_items(snapshot: &Snapshot, file: FileId, offset: TextSize) -> Vec<CompletionItem> {
    let analysis = &snapshot.compilation.analysis;
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
    if enclosing(snapshot, file, offset).is_some() {
        for keyword in SELF_KEYWORDS {
            items.push(item(keyword, CompletionItemKind::KEYWORD, "self reference"));
        }
    }
    items
}

/// The members of whatever `path` resolves to, for completion after a `.`.
fn member_items(
    snapshot: &Snapshot,
    file: FileId,
    offset: TextSize,
    path: &[String],
) -> Vec<CompletionItem> {
    let Some((root, rest)) = path.split_first() else { return Vec::new() };
    let anchor = enclosing(snapshot, file, offset);

    // `self`, `this`, and `super` are answered from the inheritance chain
    // rather than from a value, so they work before anything compiles.
    let value = match root.as_str() {
        "self" | "this" if rest.is_empty() => {
            return anchor.map(|id| property_completions(snapshot, id)).unwrap_or_default()
        }
        "super" if rest.is_empty() => {
            let Some(id) = anchor else { return Vec::new() };
            return snapshot
                .compilation
                .analysis
                .bases(id)
                .into_iter()
                .flat_map(|base| property_completions(snapshot, base))
                .collect();
        }
        "self" | "this" | "super" => {
            let Some(id) = anchor else { return Vec::new() };
            snapshot.compilation.anchor(id).map(|it| Value::Anchor(it.clone()))
        }
        name => resolve_value(snapshot, file, name),
    };

    let Some(mut value) = value else { return Vec::new() };
    for segment in rest {
        let Some(next) = value.field(segment) else { return Vec::new() };
        value = next.clone();
    }
    members_of(&value)
}

fn property_completions(snapshot: &Snapshot, anchor: piton_core::value::AnchorId) -> Vec<CompletionItem> {
    visible_properties(snapshot, anchor)
        .into_iter()
        .map(|property| {
            let detail = property
                .constraints
                .first()
                .map(|constraint| constraint.render())
                .unwrap_or_else(|| "property".to_string());
            item(&property.name, CompletionItemKind::PROPERTY, &detail)
        })
        .collect()
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

fn resolve_value(snapshot: &Snapshot, file: FileId, name: &str) -> Option<Value> {
    match snapshot.compilation.analysis.scope(file).names.get(name)? {
        Symbol::Anchor(id) => {
            snapshot.compilation.anchor(*id).map(|anchor| Value::Anchor(anchor.clone()))
        }
        Symbol::Var { file, index } => snapshot.compilation.vars.get(&(*file, *index)).cloned(),
    }
}

/// Inside an anchor body: the properties it inherits but has not written yet.
fn property_items(snapshot: &Snapshot, file: FileId, offset: TextSize) -> Vec<CompletionItem> {
    let Some(anchor) = enclosing(snapshot, file, offset) else { return Vec::new() };
    let analysis = &snapshot.compilation.analysis;
    let mut written = Vec::new();
    crate::tokens::collect(&analysis.anchor_def(anchor).body, &mut written);
    let written: Vec<String> = written.into_iter().map(|property| property.name).collect();

    visible_properties(snapshot, anchor)
        .into_iter()
        .filter(|property| !written.contains(&property.name))
        .map(|property| {
            let detail = property
                .constraints
                .first()
                .map(|constraint| constraint.render())
                .unwrap_or_else(|| "inherited property".to_string());
            let mut completion = item(&property.name, CompletionItemKind::PROPERTY, &detail);
            completion.insert_text = Some(format!("{}: ", property.name));
            completion
        })
        .collect()
}

/// The start of a top-level line: only what can begin a declaration.
fn declaration_items(
    snapshot: &Snapshot,
    file: FileId,
    after_export: bool,
    after_abstract: bool,
) -> Vec<CompletionItem> {
    let analysis = &snapshot.compilation.analysis;
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
        let definition = analysis.anchor_def(*id);
        let mut completion = item(
            keyword,
            CompletionItemKind::FUNCTION,
            &format!("shorthand for `extends {}`", definition.name),
        );
        completion.insert_text = Some(format!("{keyword} ${{1:Name}}:\n    $0"));
        completion.insert_text_format = Some(InsertTextFormat::SNIPPET);
        if let Some(doc) = &definition.doc {
            completion.documentation = Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: doc.clone(),
            }));
        }
        items.push(completion);
    }

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

/// The anchor whose body the cursor is in.
///
/// The tree answers this once the body has content. On the first line of an
/// empty body there is no block yet — the indentation is still just an empty
/// line — so the anchor is found by looking back instead.
fn enclosing(
    snapshot: &Snapshot,
    file: FileId,
    offset: TextSize,
) -> Option<piton_core::value::AnchorId> {
    let analysis = &snapshot.compilation.analysis;
    let root = analysis.db.file(file).parse.syntax();
    let node = match root.covering_element(TextRange::empty(offset)) {
        piton_syntax::NodeOrToken::Node(node) => node,
        piton_syntax::NodeOrToken::Token(token) => token.parent()?,
    };
    if let Some(anchor) = enclosing_anchor(snapshot, file, &node) {
        return Some(anchor);
    }

    let text = snapshot.text(file);
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
