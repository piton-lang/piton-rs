//! Editor analysis that the compiler does not itself report.
//!
//! Import cleanup, unused declarations, redundant overrides, inheritance and
//! composition conflicts, and the map from source to compiled output are
//! questions an editor asks of a resolved program. They are not part of
//! `piton check`: a file can be a valid description and still carry an import
//! nobody reads. The language server publishes them beside the compiler's
//! diagnostics and answers the queries the features are built on.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use piton_compile::eval::{BlockStep, Probe};
use piton_compile::{reach, Compilation, ModuleId, Symbol};
use piton_core::{
    format_number, title_case, AnchorId, Diagnostic, Label, MixedItem, Span, Value, ValueKind,
};
use piton_emit::markdown;
use piton_syntax::ast::{self, BinaryOp, BlockItem, Expr, ExprKind, FromKind, Item, MergeOp};
use piton_syntax::format::{INDENT, WRAP_COLUMN};

use crate::index::{Index, Role, Target};
use crate::world::{is_config_file, OutputSettings};

/// One correspondence between a source construct and a slice of compiled output.
#[derive(Debug, Clone)]
pub struct Mapping {
    pub source_file: PathBuf,
    pub source_span: Span,
    /// Absolute path of the compiled file this slice belongs to.
    pub output_path: PathBuf,
    pub output_start: usize,
    pub output_end: usize,
    /// `markdown`, `json`, `yaml`, or a Belay target id.
    pub adapter: String,
    /// Short label for code lenses and hover, e.g. `markdown Hello.md:1-12`.
    pub label: String,
}

/// How a `+` or `++` — or a list merge item — combines its inputs.
#[derive(Debug, Clone)]
pub struct Composition {
    pub module: ModuleId,
    pub span: Span,
    /// The property or declaration this composition belongs to, when known.
    pub enclosing: Span,
    pub summary: String,
    /// A short inlay, e.g. `+ merge`.
    pub hint: String,
}

/// One place a name is bound in a module: a declaration, or an import entry.
#[derive(Debug, Clone)]
pub struct Binding {
    /// The span of the name as the binding writes it.
    pub span: Span,
    /// The path text of the `from` declaration, or `None` for a declaration
    /// written in the module itself.
    pub from: Option<String>,
    /// The span of the whole import entry, for rewriting it.
    pub entry_span: Span,
    pub symbol: Symbol,
}

/// A name a module binds to more than one thing.
#[derive(Debug, Clone)]
pub struct Ambiguity {
    pub module: ModuleId,
    pub name: String,
    /// Every binding, in source order. `winner` indexes the one references
    /// resolve to.
    pub bindings: Vec<Binding>,
    pub winner: usize,
    /// Where the module reads the name.
    pub references: Vec<Span>,
}

/// Everything the editor features below read from one compilation.
#[derive(Debug, Default)]
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    pub compositions: Vec<Composition>,
    pub mappings: Vec<Mapping>,
    pub ambiguities: Vec<Ambiguity>,
    /// For each anchor, the anchors that copy it in with `{X}`.
    pub composers: HashMap<AnchorId, Vec<AnchorId>>,
    /// For each anchor, the anchors it copies in with `{X}`.
    pub components: HashMap<AnchorId, Vec<AnchorId>>,
}

impl Analysis {
    /// Compositions whose operator or enclosing declaration covers `offset`.
    pub fn compositions_at(&self, module: ModuleId, offset: usize) -> Vec<&Composition> {
        self.compositions
            .iter()
            .filter(|item| {
                item.module == module
                    && (item.span.contains(offset) || item.enclosing.contains(offset))
            })
            .collect()
    }

    /// Every mapping for the construct at `offset`, widest last.
    pub fn mappings_at(&self, file: &Path, offset: usize) -> Vec<&Mapping> {
        let mut found: Vec<&Mapping> = self
            .mappings
            .iter()
            .filter(|mapping| {
                paths_match(&mapping.source_file, file) && mapping.source_span.contains(offset)
            })
            .collect();
        found.sort_by_key(|mapping| mapping.source_span.len());
        found
    }

    /// The tightest mapping whose compiled slice covers `offset`.
    pub fn at_output(&self, file: &Path, offset: usize) -> Option<&Mapping> {
        self.mappings
            .iter()
            .filter(|mapping| {
                paths_match(&mapping.output_path, file)
                    && offset >= mapping.output_start
                    && offset < mapping.output_end
            })
            .min_by_key(|mapping| mapping.output_end.saturating_sub(mapping.output_start))
    }
}

/// Codes the editor fades, because deleting the declaration would not change
/// the resolved specification.
pub fn is_unnecessary(code: &str) -> bool {
    matches!(
        code,
        "unused-import"
            | "duplicate-import"
            | "broad-import"
            | "unused-anchor"
            | "unused-export"
            | "unused-variable"
            | "redundant-definition"
    )
}

/// Builds the analysis for a compiled program and its occurrence index.
pub fn analyze(compilation: &Compilation, index: &Index, output: &OutputSettings) -> Analysis {
    let mut analysis = Analysis::default();
    let reachability = reach::from_entry(compilation);

    imports(compilation, index, &mut analysis.diagnostics);
    ambiguities(compilation, index, &mut analysis);
    // Without an entry nothing is reached, so everything would read as
    // unused; the missing entry is the one thing to report.
    if compilation.resolution.has_entry {
        unused(compilation, index, &reachability, &mut analysis.diagnostics);
    }
    redundant(compilation, &mut analysis.diagnostics);
    compositions(compilation, &mut analysis);
    composition_edges(compilation, &mut analysis);
    map_outputs(compilation, output, &mut analysis.mappings);
    belay(compilation, &mut analysis);

    analysis.diagnostics.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.span.start.cmp(&right.span.start))
            .then(left.code.cmp(&right.code))
    });
    analysis
}

/// Rewrites a file's imports so only names the file references remain, in
/// canonical order. `None` when the file is already in that shape.
///
/// Star re-exports are left alone. `from ./Child export *` is how a directory
/// publishes its surface; narrowing it to the names some other file happens to
/// import would turn a public module into a private one.
pub fn organize_imports(
    compilation: &Compilation,
    index: &Index,
    module: ModuleId,
    text: &str,
) -> Option<Vec<(Span, String)>> {
    let ast = compilation.graph().get(module).ast();
    let mut edits = Vec::new();
    let mut seen_use: HashSet<String> = HashSet::new();

    for item in &ast.items {
        match item {
            Item::Use(decl) => {
                let duplicate = !seen_use.insert(decl.path.text.clone());
                let unused = !use_is_referenced(compilation, index, module, &decl.path.text);
                if duplicate || unused {
                    edits.push((line_span(text, decl.span), String::new()));
                }
            }
            Item::From(decl) if decl.kind == FromKind::Import && !decl.star => {
                let kept = kept_import_names(compilation, index, module, decl);
                let current = decl
                    .items
                    .iter()
                    .map(|entry| render_import_entry(entry))
                    .collect::<Vec<_>>();
                // A path still written with `.pi` is rewritten without it, the
                // way `piton format` writes it.
                if kept == current && !decl.path.text.ends_with(".pi") {
                    continue;
                }
                if kept.is_empty() {
                    edits.push((line_span(text, decl.span), String::new()));
                } else {
                    let rewritten = render_from(&decl.path.text, "import", &kept, false);
                    let span = line_span(text, decl.span);
                    let newline = text.as_bytes().get(span.end.saturating_sub(1)) == Some(&b'\n');
                    let replacement = if newline {
                        format!("{rewritten}\n")
                    } else {
                        rewritten
                    };
                    edits.push((span, replacement));
                }
            }
            _ => {}
        }
    }

    let changed = edits.iter().any(|(span, replacement)| {
        text.get(span.start..span.end)
            .is_some_and(|original| original != replacement)
    });
    changed.then_some(edits)
}

// ---------------------------------------------------------------------------
// Imports
// ---------------------------------------------------------------------------

fn imports(compilation: &Compilation, index: &Index, out: &mut Vec<Diagnostic>) {
    for module in compilation.graph().iter() {
        if module.is_package() {
            continue;
        }
        let path = module.path.clone();
        let ast = module.ast();
        let mut seen_use: HashMap<String, Span> = HashMap::new();

        for item in &ast.items {
            match item {
                Item::Use(decl) => {
                    if let Some(first) = seen_use.get(&decl.path.text) {
                        out.push(
                            Diagnostic::warning(
                                "duplicate-import",
                                format!("`use {}` repeats an earlier declaration", decl.path.text),
                                &path,
                                decl.span,
                            )
                            .with_label(Label::new(
                                path.clone(),
                                *first,
                                "first imported here",
                            )),
                        );
                        continue;
                    }
                    seen_use.insert(decl.path.text.clone(), decl.span);
                    if !use_is_referenced(compilation, index, module.id, &decl.path.text) {
                        let keywords = keywords_from(compilation, module.id, &decl.path.text);
                        let message = if keywords.is_empty() {
                            format!(
                                "`use {}` imports keywords, and that module declares none",
                                decl.path.text
                            )
                        } else {
                            format!(
                                "`use {}` is unused; none of its keywords are referenced",
                                decl.path.text
                            )
                        };
                        out.push(
                            Diagnostic::warning("unused-import", message, &path, decl.span)
                                .with_help(
                                    "remove it, or reference one of the keywords it brings in"
                                        .to_string(),
                                ),
                        );
                    }
                }
                Item::From(decl) if decl.kind == FromKind::Import && !decl.star => {
                    let mut seen_name: HashMap<String, Span> = HashMap::new();
                    let mut unused = 0usize;
                    let mut used = 0usize;
                    for entry in &decl.items {
                        let local = entry.local_name();
                        if let Some(first) = seen_name.get(local) {
                            out.push(
                                Diagnostic::warning(
                                    "duplicate-import",
                                    format!("`{local}` is imported more than once"),
                                    &path,
                                    entry.name_span,
                                )
                                .with_label(Label::new(
                                    path.clone(),
                                    *first,
                                    "first imported here",
                                )),
                            );
                            continue;
                        }
                        seen_name.insert(local.to_string(), entry.name_span);
                        if compilation.resolution.lookup(module.id, local).is_none() {
                            // The compiler already reports unresolved imports.
                            // Recording it again would be a second squiggle on
                            // the same name; the organize action still drops it.
                            continue;
                        }
                        if name_referenced(index, module.id, local, decl.span) {
                            used += 1;
                        } else {
                            unused += 1;
                            out.push(
                                Diagnostic::warning(
                                    "unused-import",
                                    format!("`{local}` is imported but never referenced"),
                                    &path,
                                    entry.name_span,
                                )
                                .with_help(
                                    "remove it from the import list, or reference it".to_string(),
                                ),
                            );
                        }
                    }
                    if used > 0 && unused > 0 {
                        out.push(
                            Diagnostic::warning(
                                "broad-import",
                                format!(
                                    "`from {} import` names {unused} symbol{} this file does not reference",
                                    decl.path.text,
                                    if unused == 1 { "" } else { "s" }
                                ),
                                &path,
                                decl.span,
                            )
                            .with_help(
                                "organize imports to keep only the names this file references"
                                    .to_string(),
                            ),
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

fn use_is_referenced(
    compilation: &Compilation,
    index: &Index,
    module: ModuleId,
    path_text: &str,
) -> bool {
    let Some(resolved) = resolve_written(compilation, module, path_text) else {
        return true;
    };
    let keywords = keywords_of(compilation, resolved);
    if keywords.is_empty() {
        return false;
    }
    let module_index = index.get(module);
    keywords.iter().any(|keyword| {
        module_index.is_some_and(|module_index| {
            module_index.occurrences.iter().any(|occurrence| {
                occurrence.role == Role::Reference
                    && matches!(&occurrence.target, Target::Keyword(name, _) if name == keyword)
            })
        })
    })
}

fn keywords_from(compilation: &Compilation, module: ModuleId, path_text: &str) -> Vec<String> {
    resolve_written(compilation, module, path_text)
        .map(|resolved| keywords_of(compilation, resolved))
        .unwrap_or_default()
}

fn keywords_of(compilation: &Compilation, module: ModuleId) -> Vec<String> {
    let mut out = Vec::new();
    for name in compilation.resolution.exported_names(module) {
        let Some(Symbol::Anchor(anchor)) =
            compilation
                .resolution
                .lookup_export(module, &name, &mut HashSet::new())
        else {
            continue;
        };
        if let Some(alias) = &compilation.store().anchor(anchor).alias {
            out.push(alias.clone());
        }
    }
    out
}

fn resolve_written(compilation: &Compilation, module: ModuleId, written: &str) -> Option<ModuleId> {
    let from = compilation.graph().get(module);
    let directory = from.directory();
    let context =
        piton_compile::module::ResolutionContext::new(&directory, compilation.project.roots());
    let path = piton_compile::module::resolve(written, &context).ok()?;
    compilation.graph().id_for(&path)
}

fn name_referenced(index: &Index, module: ModuleId, name: &str, import_span: Span) -> bool {
    index.get(module).is_some_and(|module_index| {
        module_index.occurrences.iter().any(|occurrence| {
            occurrence.role == Role::Reference
                && occurrence.text == name
                && !import_span.contains(occurrence.span.start)
        })
    })
}

fn kept_import_names(
    compilation: &Compilation,
    index: &Index,
    module: ModuleId,
    decl: &ast::FromDecl,
) -> Vec<String> {
    let mut kept = Vec::new();
    let mut seen = HashSet::new();
    for entry in &decl.items {
        let local = entry.local_name();
        if !seen.insert(local.to_string()) {
            continue;
        }
        if compilation.resolution.lookup(module, local).is_none() {
            continue;
        }
        if !name_referenced(index, module, local, decl.span) {
            continue;
        }
        kept.push(render_import_entry(entry));
    }
    kept.sort();
    kept
}

fn render_import_entry(entry: &ast::ImportItem) -> String {
    match &entry.alias {
        Some(alias) => format!("{} {}", entry.name, alias.value),
        None => entry.name.clone(),
    }
}

fn render_from(path: &str, keyword: &str, names: &[String], star: bool) -> String {
    // The extension is optional in an import path, and the formatter drops it.
    let path = path.strip_suffix(".pi").unwrap_or(path);
    let head = format!("from {path} {keyword}");
    if star {
        return format!("{head} *");
    }
    if names.is_empty() {
        return head;
    }
    let single = format!("{head} {}", names.join(", "));
    if names.len() > 2 || single.chars().count() > WRAP_COLUMN {
        let mut wrapped = head;
        wrapped.push('\n');
        for (index, entry) in names.iter().enumerate() {
            wrapped.push_str(&" ".repeat(INDENT));
            wrapped.push_str(entry);
            if index + 1 < names.len() {
                wrapped.push(',');
            }
            wrapped.push('\n');
        }
        wrapped.pop();
        wrapped
    } else {
        single
    }
}

// ---------------------------------------------------------------------------
// Ambiguous references
// ---------------------------------------------------------------------------

/// Names a module binds to more than one thing: two imports of the same name
/// from different modules, or an import a local declaration hides.
///
/// The compiler resolves these silently -- a declaration beats an import, and
/// the last import beats earlier ones -- so a reference can mean something
/// other than what its author had in mind. The editor says so.
fn ambiguities(compilation: &Compilation, index: &Index, analysis: &mut Analysis) {
    for module in compilation.graph().iter() {
        if module.is_package() {
            continue;
        }
        let ast = module.ast();
        let mut bindings: Vec<(String, Binding)> = Vec::new();
        let mut import_spans: Vec<Span> = Vec::new();
        for item in &ast.items {
            match item {
                Item::Anchor(decl) => {
                    if let Some(symbol) = compilation.resolution.lookup(module.id, &decl.name) {
                        bindings.push((
                            decl.name.clone(),
                            Binding {
                                span: decl.name_span,
                                from: None,
                                entry_span: decl.name_span,
                                symbol,
                            },
                        ));
                    }
                }
                Item::Variable(decl) => {
                    if let Some(symbol) = compilation.resolution.lookup(module.id, &decl.name) {
                        bindings.push((
                            decl.name.clone(),
                            Binding {
                                span: decl.name_span,
                                from: None,
                                entry_span: decl.name_span,
                                symbol,
                            },
                        ));
                    }
                }
                Item::From(decl) if decl.kind == FromKind::Import && !decl.star => {
                    import_spans.push(decl.span);
                    let Some(source) = resolve_written(compilation, module.id, &decl.path.text)
                    else {
                        continue;
                    };
                    for entry in &decl.items {
                        let Some(symbol) = compilation.resolution.lookup_export(
                            source,
                            &entry.name,
                            &mut HashSet::new(),
                        ) else {
                            continue;
                        };
                        bindings.push((
                            entry.local_name().to_string(),
                            Binding {
                                span: entry
                                    .alias
                                    .as_ref()
                                    .map(|alias| alias.span)
                                    .unwrap_or(entry.name_span),
                                from: Some(decl.path.text.clone()),
                                entry_span: entry.span,
                                symbol,
                            },
                        ));
                    }
                }
                Item::From(decl) => import_spans.push(decl.span),
                _ => {}
            }
        }

        let mut names: Vec<String> = bindings.iter().map(|(name, _)| name.clone()).collect();
        names.sort();
        names.dedup();
        for name in names {
            let mut found: Vec<Binding> = bindings
                .iter()
                .filter(|(bound, _)| *bound == name)
                .map(|(_, binding)| binding.clone())
                .collect();
            // Importing the same thing twice is a duplicate, reported as one;
            // only different things under one name are ambiguous.
            let mut distinct: Vec<Symbol> = Vec::new();
            for binding in &found {
                if !distinct.contains(&binding.symbol) {
                    distinct.push(binding.symbol);
                }
            }
            if distinct.len() < 2 {
                continue;
            }
            found.sort_by_key(|binding| binding.span.start);
            let resolved = compilation.resolution.lookup(module.id, &name);
            // The compiler prefers a declaration, then the last import.
            let winner = found
                .iter()
                .position(|binding| binding.from.is_none())
                .or_else(|| {
                    found
                        .iter()
                        .rposition(|binding| Some(binding.symbol) == resolved)
                })
                .unwrap_or(found.len() - 1);

            let references: Vec<Span> = index
                .get(module.id)
                .map(|module_index| {
                    module_index
                        .occurrences
                        .iter()
                        .filter(|occurrence| {
                            occurrence.role == Role::Reference
                                && occurrence.text == name
                                && matches!(
                                    occurrence.target,
                                    Target::Anchor(_) | Target::Variable(_)
                                )
                                && !import_spans
                                    .iter()
                                    .any(|span| span.contains(occurrence.span.start))
                        })
                        .map(|occurrence| occurrence.span)
                        .collect()
                })
                .unwrap_or_default();

            let describe = |binding: &Binding| match &binding.from {
                Some(path) => format!("`{name}` imported from `{path}`"),
                None => format!("`{name}` declared in this file"),
            };
            let meanings = found.iter().map(describe).collect::<Vec<_>>().join(" or ");
            let chosen = describe(&found[winner]);

            for span in &references {
                let mut diagnostic = Diagnostic::warning(
                    "ambiguous-reference",
                    format!("`{name}` is ambiguous: it could be {meanings}; it resolves to {chosen}"),
                    &module.path,
                    *span,
                )
                .with_help("give one of the imports an alias to say which one is meant".to_string());
                for binding in &found {
                    diagnostic = diagnostic.with_label(Label::new(
                        module.path.clone(),
                        binding.span,
                        describe(binding),
                    ));
                }
                analysis.diagnostics.push(diagnostic);
            }
            for (position, binding) in found.iter().enumerate() {
                if position == winner || binding.from.is_none() {
                    continue;
                }
                analysis.diagnostics.push(
                    Diagnostic::warning(
                        "shadowed-import",
                        format!(
                            "{} is hidden: `{name}` in this file means {chosen}",
                            describe(binding)
                        ),
                        &module.path,
                        binding.span,
                    )
                    .with_label(Label::new(
                        module.path.clone(),
                        found[winner].span,
                        "this binding wins",
                    ))
                    .with_help("alias the import, or remove it".to_string()),
                );
            }

            analysis.ambiguities.push(Ambiguity {
                module: module.id,
                name,
                bindings: found,
                winner,
                references,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Unused symbols
// ---------------------------------------------------------------------------

/// Whether a module is the project configuration, which is read by the
/// compiler rather than compiled, so nothing in it is "unused" or "output".
fn is_config_module(compilation: &Compilation, module: ModuleId) -> bool {
    is_config_file(&compilation.graph().get(module).path)
}

fn unused(
    compilation: &Compilation,
    index: &Index,
    reachability: &reach::Reachability,
    out: &mut Vec<Diagnostic>,
) {
    for anchor in &reachability.unreachable {
        let def = compilation.store().anchor(*anchor);
        let path = compilation.graph().get(def.module).path.clone();
        if path.to_string_lossy().starts_with('@') || is_config_module(compilation, def.module) {
            continue;
        }
        // A keyword that something actually writes is not dead, even when the
        // reachability walk did not land on the alias itself.
        if def.alias.as_ref().is_some_and(|alias| {
            index
                .matching(|target| matches!(target, Target::Keyword(name, _) if name == alias))
                .iter()
                .any(|(_, occurrence)| occurrence.role == Role::Reference)
        }) {
            continue;
        }
        out.push(
            Diagnostic::warning(
                "unused-anchor",
                format!(
                    "`{}` is never referenced and does not contribute to the compiled result",
                    def.name
                ),
                &path,
                def.name_span,
            )
            .with_help(
                "reference it, extend it, or remove it; `piton reach` reports the same set"
                    .to_string(),
            ),
        );
    }

    for def in &compilation.store().variables {
        let path = compilation.graph().get(def.module).path.clone();
        if path.to_string_lossy().starts_with('@') || is_config_module(compilation, def.module) {
            continue;
        }
        if reachability.modules.contains(&def.module) {
            // A variable in a module the project reaches is part of that
            // file's compiled output, whether or not an expression names it.
            continue;
        }
        let referenced = index
            .occurrences_of(&Target::Variable(def.id))
            .iter()
            .any(|(_, occurrence)| occurrence.role == Role::Reference);
        if referenced {
            continue;
        }
        let code = if def.exported {
            "unused-export"
        } else {
            "unused-variable"
        };
        out.push(
            Diagnostic::warning(
                code,
                format!(
                    "`{}` is never referenced and its module contributes nothing to the compiled result",
                    def.name
                ),
                &path,
                def.name_span,
            )
            .with_help("remove it, or reference it from a reached module".to_string()),
        );
    }

    // `export Name` that nothing, including a reached barrel, ever imports.
    for module in compilation.graph().iter() {
        if module.is_package() || is_config_module(compilation, module.id) {
            continue;
        }
        for item in &module.ast().items {
            let Item::ReExport(decl) = item else {
                continue;
            };
            let Some(symbol) = compilation.resolution.lookup(module.id, &decl.name) else {
                continue;
            };
            if symbol_is_live(compilation, reachability, symbol) {
                continue;
            }
            if index
                .occurrences_of(&target_of(symbol))
                .iter()
                .any(|(other, occurrence)| {
                    *other != module.id && occurrence.role == Role::Reference
                })
            {
                continue;
            }
            out.push(
                Diagnostic::warning(
                    "unused-export",
                    format!("`{}` is exported but nothing imports it", decl.name),
                    &module.path,
                    decl.name_span,
                )
                .with_help("remove the export, or import the name where it is needed".to_string()),
            );
        }
    }
}

fn symbol_is_live(
    compilation: &Compilation,
    reachability: &reach::Reachability,
    symbol: Symbol,
) -> bool {
    let entry = compilation.resolution.entry;
    if module_exports(compilation, entry, symbol) {
        return true;
    }
    reachability
        .modules
        .iter()
        .any(|module| module_exports(compilation, *module, symbol))
}

fn module_exports(compilation: &Compilation, module: ModuleId, symbol: Symbol) -> bool {
    compilation
        .resolution
        .exported_names(module)
        .iter()
        .any(|name| {
            compilation
                .resolution
                .lookup_export(module, name, &mut HashSet::new())
                == Some(symbol)
        })
}

fn target_of(symbol: Symbol) -> Target {
    match symbol {
        Symbol::Anchor(anchor) => Target::Anchor(anchor),
        Symbol::Variable(variable) => Target::Variable(variable),
    }
}

// ---------------------------------------------------------------------------
// Redundant definitions
// ---------------------------------------------------------------------------

/// A property that repeats, value for value, what the anchor would inherit
/// anyway can be deleted without changing the resolved specification.
///
/// A child cannot redeclare an inherited constraint (the compiler rejects
/// that), so a constraint on the declaration is no reason to keep it.
fn redundant(compilation: &Compilation, out: &mut Vec<Diagnostic>) {
    for def in &compilation.store().anchors {
        let path = compilation.graph().get(def.module).path.clone();
        if path.to_string_lossy().starts_with('@') {
            continue;
        }
        let Item::Anchor(decl) = &compilation.graph().get(def.module).ast().items[def.item] else {
            continue;
        };
        for property in decl.body.properties() {
            if !property.value.declared {
                continue;
            }
            let Some(inherited) = inherited_value(compilation, def.id, &property.name) else {
                continue;
            };
            let Some(resolved) = def.properties.get(&property.name) else {
                continue;
            };
            if resolved != inherited {
                continue;
            }
            out.push(
                Diagnostic::warning(
                    "redundant-definition",
                    format!(
                        "`{}` on `{}` repeats the inherited value and can be removed",
                        property.name, def.name
                    ),
                    &path,
                    property.name_span,
                )
                .with_help(
                    "deleting this declaration leaves the resolved specification unchanged"
                        .to_string(),
                ),
            );
        }
    }
}

/// The value the anchor would inherit for `name` without its own declaration:
/// the right-most base that has one, which is also what `super.name` reads.
fn inherited_value<'a>(
    compilation: &'a Compilation,
    anchor: AnchorId,
    name: &str,
) -> Option<&'a Value> {
    let store = compilation.store();
    store
        .anchor(anchor)
        .bases
        .iter()
        .rev()
        .find_map(|base| store.anchor(*base).properties.get(name))
}

// ---------------------------------------------------------------------------
// Composition
// ---------------------------------------------------------------------------

/// Where a composition sits, so its result can be read back from what the
/// compiler produced rather than worked out a second time.
#[derive(Clone)]
enum Holder {
    /// A property (or a key nested in one) of an anchor.
    Anchor(AnchorId, Vec<String>),
    /// A top-level variable, or a key nested in one.
    Variable(piton_compile::VariableId, Vec<String>),
    /// Somewhere no path reaches, such as inside a list.
    Nowhere,
}

impl Holder {
    fn child(&self, name: &str) -> Holder {
        match self {
            Holder::Anchor(anchor, path) => {
                let mut path = path.clone();
                path.push(name.to_string());
                Holder::Anchor(*anchor, path)
            }
            Holder::Variable(variable, path) => {
                let mut path = path.clone();
                path.push(name.to_string());
                Holder::Variable(*variable, path)
            }
            Holder::Nowhere => Holder::Nowhere,
        }
    }

    /// The compiled value at this place.
    fn value<'a>(&self, compilation: &'a Compilation) -> Option<&'a Value> {
        let (root, path) = match self {
            Holder::Anchor(anchor, path) => {
                let (first, rest) = path.split_first()?;
                (compilation.store().anchor(*anchor).properties.get(first)?, rest)
            }
            Holder::Variable(variable, path) => {
                (compilation.store().variable(*variable).value.as_ref()?, path.as_slice())
            }
            Holder::Nowhere => return None,
        };
        let mut value = root;
        for segment in path {
            value = value.property(segment, compilation)?;
        }
        Some(value)
    }
}

struct Composer<'a, 'b> {
    compilation: &'a Compilation,
    probe: Probe<'a>,
    analysis: &'b mut Analysis,
    module: ModuleId,
    owner: Option<AnchorId>,
    path: PathBuf,
}

fn compositions(compilation: &Compilation, analysis: &mut Analysis) {
    let mut composer = Composer {
        compilation,
        probe: Probe::new(&compilation.resolution),
        analysis,
        module: ModuleId(0),
        owner: None,
        path: PathBuf::new(),
    };
    for module in compilation.graph().iter() {
        if module.is_package() {
            continue;
        }
        composer.module = module.id;
        composer.path = module.path.clone();
        for (item_index, item) in module.ast().items.iter().enumerate() {
            match item {
                Item::Anchor(decl) => {
                    let Some(def) = compilation
                        .store()
                        .anchors
                        .iter()
                        .find(|def| def.module == module.id && def.item == item_index)
                    else {
                        continue;
                    };
                    composer.owner = Some(def.id);
                    for property in decl.body.properties() {
                        composer.value(
                            &property.value,
                            property.span,
                            &property.name,
                            Holder::Anchor(def.id, vec![property.name.clone()]),
                        );
                    }
                }
                Item::Variable(decl) => {
                    let Some(Symbol::Variable(variable)) =
                        compilation.resolution.lookup(module.id, &decl.name)
                    else {
                        continue;
                    };
                    composer.owner = None;
                    composer.value(
                        &decl.value,
                        decl.span,
                        &decl.name,
                        Holder::Variable(variable, Vec::new()),
                    );
                }
                _ => {}
            }
        }
    }
}

impl Composer<'_, '_> {
    fn source(&self) -> &str {
        self.compilation.graph().get(self.module).source.as_str()
    }

    fn value(&mut self, value: &ast::ValueNode, enclosing: Span, name: &str, holder: Holder) {
        if let Some(line) = &value.inline {
            self.line(line, enclosing);
        }
        if let Some(items) = &value.inline_list {
            for item in items {
                self.value(item, enclosing, name, Holder::Nowhere);
            }
        }
        let Some(block) = &value.block else {
            return;
        };
        if block
            .items
            .iter()
            .any(|item| matches!(item, BlockItem::Merge(_)))
        {
            self.block(value, block, enclosing, name, &holder);
        }
        for item in &block.items {
            match item {
                BlockItem::Property(property) => self.value(
                    &property.value,
                    property.span,
                    &property.name,
                    holder.child(&property.name),
                ),
                BlockItem::ListItem(entry) => {
                    self.value(&entry.value, enclosing, name, Holder::Nowhere)
                }
                BlockItem::Prose(paragraph) => {
                    for line in &paragraph.lines {
                        self.line(line, enclosing);
                    }
                }
                BlockItem::Merge(merge) => self.line(&merge.value, enclosing),
                _ => {}
            }
        }
    }

    fn line(&mut self, line: &ast::ProseLine, enclosing: Span) {
        for segment in &line.segments {
            if let ast::ProseSegment::Interpolation(interpolation) = segment {
                self.expr(&interpolation.expr, enclosing);
            }
        }
    }

    fn expr(&mut self, expr: &Expr, enclosing: Span) {
        if let ExprKind::Binary(op @ (BinaryOp::Add | BinaryOp::Concat), left, right) = &expr.kind
        {
            self.binary(expr, *op, left, right, enclosing);
        }
        match &expr.kind {
            ExprKind::Field(base, _) => self.expr(base, enclosing),
            ExprKind::Unary(_, operand) | ExprKind::Paren(operand) | ExprKind::Nested(_, operand) => {
                self.expr(operand, enclosing)
            }
            ExprKind::Binary(_, left, right) => {
                self.expr(left, enclosing);
                self.expr(right, enclosing);
            }
            ExprKind::Ternary(condition, consequent, alternative) => {
                self.expr(condition, enclosing);
                self.expr(consequent, enclosing);
                self.expr(alternative, enclosing);
            }
            ExprKind::List(items) => {
                for item in items {
                    self.expr(item, enclosing);
                }
            }
            _ => {}
        }
    }

    /// `{a + b}` or `{a ++ b}`: both inputs and the result, each evaluated by
    /// the compiler's own rules.
    fn binary(&mut self, whole: &Expr, op: BinaryOp, left: &Expr, right: &Expr, enclosing: Span) {
        let left_value = self.probe.expr(self.module, self.owner, left);
        let right_value = self.probe.expr(self.module, self.owner, right);
        let combined = match (&left_value, &right_value) {
            (Some(_), Some(_)) => self.probe.expr(self.module, self.owner, whole),
            _ => None,
        };
        let source = self.source().to_string();
        let rule = match (&left_value, &right_value) {
            (Some(left), Some(right)) => rule(op, left, right),
            _ => Rule::unknown(op),
        };
        let mut out = format!("**Composition** — {}\n\n", rule.explanation);
        out.push_str(&format!("1. `{}`", snippet(&source, left.span)));
        if let Some(value) = &left_value {
            out.push_str(&format!(" → {}", short(self.compilation, value)));
        }
        out.push_str(&format!("\n2. `{}`", snippet(&source, right.span)));
        if let Some(value) = &right_value {
            out.push_str(&format!(" → {}", short(self.compilation, value)));
        }
        out.push_str("\n\n");
        match &combined {
            Some(value) => {
                out.push_str("Result:\n\n```\n");
                out.push_str(&preview(self.compilation, value));
                out.push_str("\n```");
            }
            // The compiler reports why; the hover only has to say it failed.
            None if left_value.is_some() && right_value.is_some() => out.push_str(
                "_These inputs do not combine under this operator; see the compiler's error._",
            ),
            None => {}
        }
        self.analysis.compositions.push(Composition {
            module: self.module,
            span: whole.span,
            enclosing,
            summary: out,
            hint: rule.hint,
        });
    }

    /// A block with `+`/`++` lines, built from top to bottom.
    fn block(
        &mut self,
        node: &ast::ValueNode,
        block: &ast::Block,
        enclosing: Span,
        name: &str,
        holder: &Holder,
    ) {
        let steps = self.probe.block_steps(self.module, self.owner, node);
        if steps.is_empty() {
            return;
        }
        let source = self.source().to_string();
        let mut out = format!(
            "**Composition** — `{name}` is built from top to bottom: each `+` or `++` line combines everything above it with its own value.\n\n"
        );
        let mut hints: Vec<String> = Vec::new();
        for (number, step) in steps.iter().enumerate() {
            let rule = step_rule(step);
            out.push_str(&format!(
                "{}. `{}` — {}\n",
                number + 1,
                snippet(&source, step.span),
                rule.explanation
            ));
            if let Some(before) = &step.before {
                out.push_str(&format!(
                    "   above: {} · this line: {} → {}\n",
                    short(self.compilation, before),
                    short(self.compilation, &step.operand),
                    short(self.compilation, &step.after)
                ));
            } else {
                out.push_str(&format!(
                    "   this line: {}\n",
                    short(self.compilation, &step.operand)
                ));
            }
            if !hints.contains(&rule.hint) {
                hints.push(rule.hint.clone());
            }
            // A line that leaves the result exactly as it was contributes
            // nothing, whatever it says.
            if step.before.as_ref() == Some(&step.after) {
                self.analysis.diagnostics.push(
                    Diagnostic::warning(
                        "redundant-definition",
                        "this line does not change the result and can be removed",
                        &self.path,
                        step.span,
                    )
                    .with_help(match step.op {
                        MergeOp::Merge => {
                            "everything it adds is already there once `+` has combined them"
                                .to_string()
                        }
                        MergeOp::Concat => "combining with it leaves the value as it was".to_string(),
                    }),
                );
            }
        }
        let result = holder
            .value(self.compilation)
            .cloned()
            .or_else(|| steps.last().map(|step| step.after.clone()));
        if let Some(value) = result {
            out.push_str("\nResult:\n\n```\n");
            out.push_str(&preview(self.compilation, &value));
            out.push_str("\n```");
        }
        let span = block
            .items
            .iter()
            .filter_map(|item| match item {
                BlockItem::Merge(merge) => Some(merge.span),
                _ => None,
            })
            .reduce(|left, right| left.cover(right))
            .unwrap_or(enclosing);
        self.analysis.compositions.push(Composition {
            module: self.module,
            span,
            enclosing,
            summary: out,
            hint: hints.join(", "),
        });
    }
}

/// What an operator does with a pair of inputs, in words and as an inlay.
struct Rule {
    explanation: String,
    hint: String,
}

impl Rule {
    fn new(explanation: &str, hint: &str) -> Rule {
        Rule {
            explanation: explanation.to_string(),
            hint: hint.to_string(),
        }
    }

    fn unknown(op: BinaryOp) -> Rule {
        match op {
            BinaryOp::Concat => Rule::new("`++`: what it does depends on the kinds of its inputs", "++"),
            _ => Rule::new("`+`: what it does depends on the kinds of its inputs", "+"),
        }
    }
}

fn is_list(value: &Value) -> bool {
    matches!(value.kind(), ValueKind::List)
}

/// The rule the compiler applies to `left op right`, by the kinds of the two.
fn rule(op: BinaryOp, left: &Value, right: &Value) -> Rule {
    let string = |value: &Value| matches!(value, Value::Str(_));
    match op {
        BinaryOp::Add => {
            if matches!((left, right), (Value::Number(_), Value::Number(_))) {
                Rule::new("`+` adds the two numbers", "+ add")
            } else if is_list(left) && is_list(right) {
                Rule::new(
                    "`+` merges the lists: they join in order, and a value that appears more than once is kept only where it appears last",
                    "+ merge",
                )
            } else if matches!((left, right), (Value::Dict(_), Value::Dict(_))) {
                Rule::new(
                    "`+` merges the dictionaries shallowly: keys from both, the right side wins a shared key",
                    "+ shallow merge",
                )
            } else if (string(left) || string(right)) && left.is_simple() && right.is_simple() {
                Rule::new(
                    "`+` joins the strings with nothing in between (a number, boolean or null becomes text first)",
                    "+ join",
                )
            } else if (string(left) && right.is_complex()) || (left.is_complex() && string(right)) {
                Rule::new(
                    "`+` of text and a list, dictionary or anchor makes an implicit list of the two",
                    "+ implicit list",
                )
            } else {
                Rule::new(
                    &format!("`+` cannot combine {} and {}", left.kind(), right.kind()),
                    "+ ✗",
                )
            }
        }
        BinaryOp::Concat => {
            if is_list(left) && is_list(right) {
                Rule::new(
                    "`++` concatenates the lists in order and keeps duplicates",
                    "++ concat",
                )
            } else if matches!((left, right), (Value::Dict(_), Value::Dict(_))) {
                Rule::new(
                    "`++` merges the dictionaries deeply: nested dictionaries under a shared key merge too, otherwise the right side wins",
                    "++ deep merge",
                )
            } else if (string(left) || string(right)) && left.is_simple() && right.is_simple() {
                Rule::new("`++` joins the strings with a line break between them", "++ join")
            } else {
                Rule::new(
                    &format!("`++` cannot combine {} and {}", left.kind(), right.kind()),
                    "++ ✗",
                )
            }
        }
        _ => Rule::new("composition", ""),
    }
}

/// The rule for one block line, which has the block above it as its left side.
fn step_rule(step: &BlockStep) -> Rule {
    let op = match step.op {
        MergeOp::Merge => BinaryOp::Add,
        MergeOp::Concat => BinaryOp::Concat,
    };
    match &step.before {
        None => Rule::new(
            &format!(
                "`{}` with nothing above it: the block starts as this value",
                op.symbol()
            ),
            op.symbol(),
        ),
        Some(before) => {
            // A single value combined into a list is one more item.
            let right = if is_list(before) && !is_list(&step.operand) {
                Value::List(vec![step.operand.clone()])
            } else {
                step.operand.clone()
            };
            rule(op, before, &right)
        }
    }
}

/// A one-line rendering of a value for a hover step.
fn short(compilation: &Compilation, value: &Value) -> String {
    fn walk(compilation: &Compilation, value: &Value, out: &mut String, budget: &mut usize) {
        if *budget == 0 {
            return;
        }
        let before = out.len();
        match value {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Number(n) => out.push_str(&format_number(*n)),
            Value::Str(text) => {
                let plain = text.render_plain(compilation).replace('\n', "⏎");
                let clipped: String = plain.chars().take(48).collect();
                out.push('"');
                out.push_str(&clipped);
                if plain.chars().count() > 48 {
                    out.push('…');
                }
                out.push('"');
            }
            Value::List(_) | Value::Mixed(_) => {
                out.push('[');
                for (index, item) in value.as_list_items().iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    if *budget == 0 || index >= 8 {
                        out.push('…');
                        break;
                    }
                    walk(compilation, item, out, budget);
                }
                out.push(']');
            }
            Value::Dict(map) => {
                out.push('{');
                for (index, (key, item)) in map.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    if *budget == 0 || index >= 6 {
                        out.push('…');
                        break;
                    }
                    out.push_str(key);
                    out.push_str(": ");
                    walk(compilation, item, out, budget);
                }
                out.push('}');
            }
            Value::Anchor(anchor) => {
                out.push('{');
                out.push_str(&compilation.store().anchor(*anchor).name);
                out.push('}');
            }
            Value::Reference(target) => {
                out.push_str("@{");
                out.push_str(&target.display(compilation));
                out.push('}');
            }
        }
        *budget = budget.saturating_sub(out.len() - before);
    }
    let mut out = String::new();
    let mut budget = 160usize;
    walk(compilation, value, &mut out, &mut budget);
    out
}

fn preview(compilation: &Compilation, value: &Value) -> String {
    let context = markdown::Context {
        anchors: compilation,
        links: &markdown::NoLinks,
    };
    let rendered = markdown::body(value, 1, &context);
    let trimmed = rendered.trim_end();
    if trimmed.chars().count() <= 400 {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(400).collect();
    format!("{truncated}\n…")
}

fn snippet(source: &str, span: Span) -> String {
    source
        .get(span.start..span.end.min(source.len()))
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Composition edges
// ---------------------------------------------------------------------------

/// Which anchors copy which others in with `{X}`.
///
/// Only values an anchor writes itself count: an inherited property that
/// embeds something is the base's composition, not the child's.
fn composition_edges(compilation: &Compilation, analysis: &mut Analysis) {
    fn collect(value: &Value, out: &mut Vec<AnchorId>) {
        match value {
            Value::Anchor(anchor) => {
                if !out.contains(anchor) {
                    out.push(*anchor);
                }
            }
            Value::List(items) => items.iter().for_each(|item| collect(item, out)),
            Value::Dict(map) => map.values().for_each(|item| collect(item, out)),
            Value::Mixed(mixed) => {
                for item in &mixed.items {
                    match item {
                        MixedItem::List(items) => items.iter().for_each(|item| collect(item, out)),
                        MixedItem::Entry(_, value) | MixedItem::Value(value) => collect(value, out),
                        MixedItem::Text(_) => {}
                    }
                }
            }
            _ => {}
        }
    }
    for def in &compilation.store().anchors {
        let mut embedded = Vec::new();
        for (name, slot) in &def.slots {
            if slot.owner != def.id {
                continue;
            }
            if let Some(value) = def.properties.get(name) {
                collect(value, &mut embedded);
            }
        }
        embedded.retain(|anchor| *anchor != def.id);
        if embedded.is_empty() {
            continue;
        }
        for component in &embedded {
            analysis
                .composers
                .entry(*component)
                .or_default()
                .push(def.id);
        }
        analysis.components.insert(def.id, embedded);
    }
}

// ---------------------------------------------------------------------------
// Source to output
// ---------------------------------------------------------------------------

/// Maps every compiled module to the file `piton compile` writes for it: the
/// configured output directory and renderer.
fn map_outputs(compilation: &Compilation, output: &OutputSettings, mappings: &mut Vec<Mapping>) {
    for module in compilation.graph().iter() {
        if module.is_package() || is_config_module(compilation, module.id) {
            continue;
        }
        let declarations = file_declarations(compilation, module.id);
        if declarations.is_empty() {
            continue;
        }
        let adapter = output.renderer;
        let output_path = output.output_for(&compilation.project, &module.path);
        let directory = output_path.parent().unwrap_or(Path::new("."));
        let context = piton_emit::MarkdownContext {
            from_directory: directory,
            source_root: &compilation.project.source_root,
        };
        let rendered = piton_emit::render(adapter, &declarations, compilation, context);
        match adapter {
            piton_emit::Adapter::Markdown => map_markdown(
                compilation,
                &module.path,
                &output_path,
                &rendered,
                &declarations,
                mappings,
            ),
            piton_emit::Adapter::Json | piton_emit::Adapter::Yaml => map_structured(
                compilation,
                &module.path,
                &output_path,
                adapter.as_str(),
                &rendered,
                &declarations,
                mappings,
            ),
        }
    }
}

/// What a file compiles to, which is what the mapping has to describe: the
/// same surface `piton compile` renders -- its exports.
pub fn file_declarations(compilation: &Compilation, module: ModuleId) -> piton_core::Properties {
    compilation.compiled_surface(module)
}

fn map_markdown(
    compilation: &Compilation,
    source: &Path,
    output: &Path,
    rendered: &str,
    declarations: &piton_core::Properties,
    mappings: &mut Vec<Mapping>,
) {
    let end = rendered.len();
    let mut cursor = 0usize;
    for (name, value) in declarations {
        let heading = format!("# {}", title_case(name));
        let Some(relative) = rendered[cursor..end].find(&heading) else {
            continue;
        };
        let start = cursor + relative;
        let after = start + heading.len();
        let next = rendered[after..end]
            .find("\n# ")
            .map(|index| after + index)
            .unwrap_or(end);
        if let Some((origin, span)) = declaration_origin(compilation, source, name) {
            push_mapping(
                mappings,
                &origin,
                span,
                output,
                start,
                next,
                "markdown",
                &format!(
                    "markdown {}:{}-{}",
                    output
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("output"),
                    line_of(rendered, start),
                    line_of(rendered, next.saturating_sub(1))
                ),
            );
        }
        if let Value::Anchor(anchor) = value {
            map_property_headings(
                compilation,
                output,
                "markdown",
                rendered,
                after,
                next,
                *anchor,
                2,
                mappings,
            );
        }
        cursor = after;
    }
}

fn map_property_headings(
    compilation: &Compilation,
    output: &Path,
    adapter: &str,
    rendered: &str,
    from: usize,
    to: usize,
    anchor: AnchorId,
    level: usize,
    mappings: &mut Vec<Mapping>,
) {
    let def = compilation.store().anchor(anchor);
    let module = compilation.graph().get(def.module);
    let Item::Anchor(decl) = &module.ast().items[def.item] else {
        return;
    };
    let source = module.path.clone();
    let source = source.as_path();
    let marks = "#".repeat(level);
    let mut cursor = from;
    for property in decl.body.properties() {
        let heading = format!("{marks} {}", title_case(&property.name));
        let Some(relative) = rendered[cursor..to].find(&heading) else {
            continue;
        };
        let start = cursor + relative;
        let after = start + heading.len();
        let boundary = format!("\n{marks} ");
        let next = rendered[after..to]
            .find(&boundary)
            .map(|index| after + index)
            .unwrap_or(to);
        push_mapping(
            mappings,
            source,
            property.span,
            output,
            start,
            next,
            adapter,
            &format!(
                "{adapter} {}:{}-{}",
                output
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("output"),
                line_of(rendered, start),
                line_of(rendered, next.saturating_sub(1))
            ),
        );
        cursor = after;
    }
}

fn map_structured(
    compilation: &Compilation,
    source: &Path,
    output: &Path,
    adapter: &str,
    rendered: &str,
    declarations: &piton_core::Properties,
    mappings: &mut Vec<Mapping>,
) {
    let mut cursor = 0usize;
    for (name, value) in declarations {
        let key = if adapter == "json" {
            format!("\"{name}\"")
        } else {
            format!("{name}:")
        };
        let Some(relative) = rendered[cursor..].find(&key) else {
            continue;
        };
        let start = cursor + relative;
        let after = start + key.len();
        let next_key = declarations
            .keys()
            .skip_while(|candidate| *candidate != name)
            .nth(1)
            .and_then(|next| {
                let pattern = if adapter == "json" {
                    format!("\"{next}\"")
                } else {
                    format!("\n{next}:")
                };
                rendered[after..].find(&pattern).map(|index| after + index)
            })
            .unwrap_or(rendered.len());
        if let Some((origin, span)) = declaration_origin(compilation, source, name) {
            push_mapping(
                mappings,
                &origin,
                span,
                output,
                start,
                next_key,
                adapter,
                &format!(
                    "{adapter} {}:{}",
                    file_name(output),
                    line_of(rendered, start)
                ),
            );
        }
        if let Value::Anchor(anchor) = value {
            map_structured_properties(
                compilation,
                output,
                adapter,
                rendered,
                after,
                next_key,
                *anchor,
                mappings,
            );
        }
        cursor = after;
    }
}

fn map_structured_properties(
    compilation: &Compilation,
    output: &Path,
    adapter: &str,
    rendered: &str,
    from: usize,
    to: usize,
    anchor: AnchorId,
    mappings: &mut Vec<Mapping>,
) {
    let def = compilation.store().anchor(anchor);
    let module = compilation.graph().get(def.module);
    let Item::Anchor(decl) = &module.ast().items[def.item] else {
        return;
    };
    let source = module.path.clone();
    let source = source.as_path();
    let slice = &rendered[from..to];
    for property in decl.body.properties() {
        let key = if adapter == "json" {
            format!("\"{}\"", property.name)
        } else {
            format!("{}:", property.name)
        };
        let Some(relative) = slice.find(&key) else {
            continue;
        };
        let start = from + relative;
        push_mapping(
            mappings,
            source,
            property.name_span,
            output,
            start,
            (start + key.len()).min(to),
            adapter,
            &format!(
                "{adapter} {}:{}",
                file_name(output),
                line_of(rendered, start)
            ),
        );
    }
}

/// Where a name a file compiles was actually written.
///
/// A file compiles what it exports as well as what it declares, so a name in
/// its output may have been declared somewhere else and only forwarded here.
/// The mapping has to point at the declaration, wherever it lives, or it would
/// send someone to a span in a file that never wrote it.
fn declaration_origin(
    compilation: &Compilation,
    source: &Path,
    name: &str,
) -> Option<(PathBuf, Span)> {
    let module = compilation.graph().id_for(source)?;
    let symbol = compilation.resolution.lookup(module, name).or_else(|| {
        compilation
            .resolution
            .lookup_export(module, name, &mut Default::default())
    })?;
    Some(match symbol {
        Symbol::Anchor(anchor) => {
            let def = compilation.store().anchor(anchor);
            (
                compilation.graph().get(def.module).path.clone(),
                def.name_span,
            )
        }
        Symbol::Variable(variable) => {
            let def = compilation.store().variable(variable);
            (
                compilation.graph().get(def.module).path.clone(),
                def.name_span,
            )
        }
    })
}

fn push_mapping(
    mappings: &mut Vec<Mapping>,
    source: &Path,
    span: Span,
    output: &Path,
    start: usize,
    end: usize,
    adapter: &str,
    label: &str,
) {
    if end <= start || span.is_empty() {
        return;
    }
    mappings.push(Mapping {
        source_file: source.to_path_buf(),
        source_span: span,
        output_path: output.to_path_buf(),
        output_start: start,
        output_end: end,
        adapter: adapter.to_string(),
        label: label.to_string(),
    });
}

fn belay(compilation: &Compilation, analysis: &mut Analysis) {
    let Some(config) = compilation.project.belay() else {
        return;
    };
    let plan = piton_belay::plan(compilation, config);
    for diagnostic in plan.diagnostics {
        analysis
            .diagnostics
            .push(place_belay(compilation, diagnostic));
    }
    for file in &plan.files {
        let Some(anchor) = anchor_for_output(compilation, file) else {
            continue;
        };
        let def = compilation.store().anchor(anchor);
        let source = compilation.graph().get(def.module).path.clone();
        if source.to_string_lossy().starts_with('@') {
            continue;
        }
        let output = compilation.project.root.join(&file.path);
        let label = format!("{} {}", file.target, file.path.display());
        push_mapping(
            &mut analysis.mappings,
            &source,
            def.span,
            &output,
            0,
            file.contents.len().max(1),
            file.target,
            &label,
        );
        map_property_headings(
            compilation,
            &output,
            file.target,
            &file.contents,
            0,
            file.contents.len(),
            anchor,
            2,
            &mut analysis.mappings,
        );
    }
}

fn anchor_for_output(
    compilation: &Compilation,
    file: &piton_belay::OutputFile,
) -> Option<AnchorId> {
    compilation
        .store()
        .anchors
        .iter()
        .find(|def| {
            def.name == file.origin
                && file
                    .sources
                    .iter()
                    .any(|source| compilation.graph().get(def.module).path == *source)
        })
        .map(|def| def.id)
        .or_else(|| compilation.find_anchor(&file.origin))
}

/// Plan diagnostics are raised against the configuration, which is not the
/// construct the editor is looking at. When the plan names an origin, the
/// squiggle belongs on that anchor.
fn place_belay(compilation: &Compilation, mut diagnostic: Diagnostic) -> Diagnostic {
    let Some(origin) = diagnostic.origin.clone() else {
        return diagnostic;
    };
    let name = origin.split('.').next().unwrap_or(&origin);
    let Some(anchor) = compilation.find_anchor(name) else {
        return diagnostic;
    };
    let def = compilation.store().anchor(anchor);
    let path = compilation.graph().get(def.module).path.clone();
    if path.to_string_lossy().starts_with('@') {
        return diagnostic;
    }
    if diagnostic.span.is_empty() {
        diagnostic.span = def.name_span;
        diagnostic.file = path;
    }
    diagnostic
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("output")
        .to_string()
}

fn line_span(text: &str, span: Span) -> Span {
    let start = text[..span.start.min(text.len())]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let end = text[span.end.min(text.len())..]
        .find('\n')
        .map(|index| span.end + index + 1)
        .unwrap_or(text.len());
    Span::new(start, end)
}

fn paths_match(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    // A query may hand back a canonical path, or the path as the editor
    // spelled it. Comparing the lossy form catches the separator difference
    // without requiring the file to exist.
    left.to_string_lossy().replace('\\', "/") == right.to_string_lossy().replace('\\', "/")
}
