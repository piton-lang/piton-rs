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

use piton_compile::{reach, Compilation, ModuleId, Symbol};
use piton_core::{title_case, AnchorId, Diagnostic, Label, Severity, Span, Value, ValueKind};
use piton_emit::markdown;
use piton_syntax::ast::{self, BinaryOp, BlockItem, Expr, ExprKind, FromKind, Item, MergeOp};
use piton_syntax::format::{INDENT, WRAP_COLUMN};

use crate::index::{Index, Role, Target};

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

/// Everything the editor features below read from one compilation.
#[derive(Debug, Default)]
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    pub compositions: Vec<Composition>,
    pub mappings: Vec<Mapping>,
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

    /// The tightest mapping whose source span covers `offset`.
    pub fn at_source(&self, file: &Path, offset: usize) -> Option<&Mapping> {
        self.mappings
            .iter()
            .filter(|mapping| {
                paths_match(&mapping.source_file, file) && mapping.source_span.contains(offset)
            })
            .min_by_key(|mapping| mapping.source_span.len())
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
pub fn analyze(compilation: &Compilation, index: &Index) -> Analysis {
    let mut analysis = Analysis::default();
    let reachability = reach::from_entry(compilation);

    imports(compilation, index, &mut analysis.diagnostics);
    unused(compilation, index, &reachability, &mut analysis.diagnostics);
    redundant(compilation, &mut analysis.diagnostics);
    conflicts(compilation, &mut analysis);
    map_outputs(compilation, &mut analysis.mappings);
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
                if kept == current {
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
// Unused symbols
// ---------------------------------------------------------------------------

fn unused(
    compilation: &Compilation,
    index: &Index,
    reachability: &reach::Reachability,
    out: &mut Vec<Diagnostic>,
) {
    for anchor in &reachability.unreachable {
        let def = compilation.store().anchor(*anchor);
        let path = compilation.graph().get(def.module).path.clone();
        if path.to_string_lossy().starts_with('@') {
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
        if path.to_string_lossy().starts_with('@') {
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
        if module.is_package() {
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
            // A declaration that tightens the inherited constraints is not
            // dead even when today's value would have satisfied both.
            if !property.constraints.is_empty() {
                let inherited_constraints =
                    inherited_constraints(compilation, def.id, &property.name);
                if property.constraints != inherited_constraints {
                    continue;
                }
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

/// The value the right-most base would contribute for `name`, if any base has it.
fn inherited_value<'a>(
    compilation: &'a Compilation,
    anchor: AnchorId,
    name: &str,
) -> Option<&'a Value> {
    let mut found = None;
    for base in compilation.store().base_chain(anchor) {
        if base == anchor {
            break;
        }
        if let Some(value) = compilation.store().anchor(base).properties.get(name) {
            found = Some(value);
        }
    }
    found
}

fn inherited_constraints(
    compilation: &Compilation,
    anchor: AnchorId,
    name: &str,
) -> Vec<piton_syntax::ast::TypeConstraint> {
    let mut found = Vec::new();
    for base in compilation.store().base_chain(anchor) {
        if base == anchor {
            break;
        }
        if let Some(slot) = compilation.store().anchor(base).slots.get(name) {
            if !slot.constraints.is_empty() {
                found = slot.constraints.clone();
            }
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Conflicts and composition
// ---------------------------------------------------------------------------

struct Contribution {
    owner: AnchorId,
    constraints: Vec<ast::TypeConstraint>,
}

fn conflicts(compilation: &Compilation, analysis: &mut Analysis) {
    for def in compilation.store().anchors.clone() {
        let path = compilation.graph().get(def.module).path.clone();
        if path.to_string_lossy().starts_with('@') {
            continue;
        }
        let Item::Anchor(decl) = &compilation.graph().get(def.module).ast().items[def.item] else {
            continue;
        };
        let decl = decl.clone();

        inheritance_conflicts(compilation, &def, &decl, &path, &mut analysis.diagnostics);
        walk_anchor_compositions(compilation, &def, &decl, &path, analysis);
    }
}

fn inheritance_conflicts(
    compilation: &Compilation,
    def: &piton_compile::store::AnchorDef,
    decl: &ast::AnchorDecl,
    path: &Path,
    out: &mut Vec<Diagnostic>,
) {
    let mut contributed: HashMap<String, Vec<Contribution>> = HashMap::new();
    for base in &def.bases {
        for (name, slot) in &compilation.store().anchor(*base).slots {
            let entry = contributed.entry(name.clone()).or_default();
            if entry.iter().any(|item| item.owner == slot.owner) {
                continue;
            }
            entry.push(Contribution {
                owner: slot.owner,
                constraints: slot.constraints.clone(),
            });
        }
    }

    for (name, sources) in contributed {
        let constrained: Vec<&Contribution> = sources
            .iter()
            .filter(|item| !item.constraints.is_empty())
            .collect();
        if constrained.len() < 2 {
            continue;
        }
        let disagrees = constrained.iter().any(|left| {
            constrained.iter().any(|right| {
                left.owner != right.owner && incompatible(&left.constraints, &right.constraints)
            })
        });
        if !disagrees {
            continue;
        }
        if decl
            .body
            .properties()
            .any(|property| property.name == name && !property.constraints.is_empty())
        {
            // The child named the constraint, so the disagreement is resolved.
            continue;
        }
        let abstracts = constrained
            .iter()
            .filter(|item| compilation.store().anchor(item.owner).is_abstract)
            .count();
        let severity = if abstracts >= 2 {
            Severity::Error
        } else {
            Severity::Warning
        };
        let described = constrained
            .iter()
            .map(|item| {
                let owner = compilation.store().anchor(item.owner);
                let labels = item
                    .constraints
                    .iter()
                    .map(piton_compile::eval::constraint_label)
                    .collect::<Vec<_>>()
                    .join(" or ");
                format!("`{}` ({labels})", owner.name)
            })
            .collect::<Vec<_>>()
            .join(" and ");
        let span = decl
            .body
            .properties()
            .find(|property| property.name == name)
            .map(|property| property.name_span)
            .unwrap_or(def.name_span);
        let mut diagnostic = Diagnostic::new(
            severity,
            "inheritance-conflict",
            format!(
                "`{}` inherits `{name}` from {described}, and those constraints cannot be resolved together",
                def.name
            ),
            path,
            span,
        )
        .with_help(
            "declare the constraint on this anchor, or drop one of the bases".to_string(),
        );
        for item in constrained {
            let owner = compilation.store().anchor(item.owner);
            let owner_path = compilation.graph().get(owner.module).path.clone();
            diagnostic = diagnostic.with_label(Label::new(
                owner_path,
                item.owner_span(compilation, &name),
                format!("`{name}` constrained here"),
            ));
        }
        out.push(diagnostic);
    }
}

impl Contribution {
    fn owner_span(&self, compilation: &Compilation, name: &str) -> Span {
        compilation
            .store()
            .anchor(self.owner)
            .slots
            .get(name)
            .map(|slot| slot.span)
            .unwrap_or_else(|| compilation.store().anchor(self.owner).name_span)
    }
}

fn incompatible(left: &[ast::TypeConstraint], right: &[ast::TypeConstraint]) -> bool {
    !left.iter().any(|constraint| {
        right.iter().any(|other| {
            piton_compile::eval::constraint_label(constraint)
                == piton_compile::eval::constraint_label(other)
        })
    })
}

fn walk_anchor_compositions(
    compilation: &Compilation,
    def: &piton_compile::store::AnchorDef,
    decl: &ast::AnchorDecl,
    path: &Path,
    analysis: &mut Analysis,
) {
    for property in decl.body.properties() {
        walk_value(
            compilation,
            def.module,
            property.span,
            Some(def.id),
            &property.value,
            path,
            analysis,
        );
    }
}

fn walk_value(
    compilation: &Compilation,
    module: ModuleId,
    enclosing: Span,
    owner: Option<AnchorId>,
    value: &ast::ValueNode,
    path: &Path,
    analysis: &mut Analysis,
) {
    if let Some(line) = &value.inline {
        walk_prose(compilation, module, enclosing, owner, line, path, analysis);
    }
    if let Some(items) = &value.inline_list {
        for item in items {
            walk_value(compilation, module, enclosing, owner, item, path, analysis);
        }
    }
    if let Some(block) = &value.block {
        walk_list_merges(compilation, module, enclosing, owner, block, path, analysis);
        for item in &block.items {
            match item {
                BlockItem::Property(property) => {
                    walk_value(
                        compilation,
                        module,
                        property.span,
                        owner,
                        &property.value,
                        path,
                        analysis,
                    );
                }
                BlockItem::ListItem(entry) => {
                    walk_value(
                        compilation,
                        module,
                        enclosing,
                        owner,
                        &entry.value,
                        path,
                        analysis,
                    );
                }
                BlockItem::Prose(paragraph) => {
                    for line in &paragraph.lines {
                        walk_prose(compilation, module, enclosing, owner, line, path, analysis);
                    }
                }
                BlockItem::Merge(merge) => {
                    walk_prose(
                        compilation,
                        module,
                        enclosing,
                        owner,
                        &merge.value,
                        path,
                        analysis,
                    );
                }
                _ => {}
            }
        }
    }
}

fn walk_prose(
    compilation: &Compilation,
    module: ModuleId,
    enclosing: Span,
    owner: Option<AnchorId>,
    line: &ast::ProseLine,
    path: &Path,
    analysis: &mut Analysis,
) {
    for segment in &line.segments {
        if let ast::ProseSegment::Interpolation(interpolation) = segment {
            walk_expr(
                compilation,
                module,
                enclosing,
                owner,
                &interpolation.expr,
                path,
                analysis,
            );
        }
    }
}

fn walk_expr(
    compilation: &Compilation,
    module: ModuleId,
    enclosing: Span,
    owner: Option<AnchorId>,
    expr: &Expr,
    path: &Path,
    analysis: &mut Analysis,
) {
    if let ExprKind::Binary(op @ (BinaryOp::Add | BinaryOp::Concat), left, right) = &expr.kind {
        record_binary(
            compilation,
            module,
            enclosing,
            owner,
            expr.span,
            *op,
            left,
            right,
            path,
            analysis,
        );
    }
    match &expr.kind {
        ExprKind::Field(base, _) => {
            walk_expr(compilation, module, enclosing, owner, base, path, analysis)
        }
        ExprKind::Unary(_, operand) | ExprKind::Paren(operand) => {
            walk_expr(
                compilation,
                module,
                enclosing,
                owner,
                operand,
                path,
                analysis,
            );
        }
        ExprKind::Binary(_, left, right) => {
            walk_expr(compilation, module, enclosing, owner, left, path, analysis);
            walk_expr(compilation, module, enclosing, owner, right, path, analysis);
        }
        ExprKind::Ternary(condition, consequent, alternative) => {
            walk_expr(
                compilation,
                module,
                enclosing,
                owner,
                condition,
                path,
                analysis,
            );
            walk_expr(
                compilation,
                module,
                enclosing,
                owner,
                consequent,
                path,
                analysis,
            );
            walk_expr(
                compilation,
                module,
                enclosing,
                owner,
                alternative,
                path,
                analysis,
            );
        }
        ExprKind::List(items) => {
            for item in items {
                walk_expr(compilation, module, enclosing, owner, item, path, analysis);
            }
        }
        _ => {}
    }
}

fn record_binary(
    compilation: &Compilation,
    module: ModuleId,
    enclosing: Span,
    owner: Option<AnchorId>,
    span: Span,
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
    path: &Path,
    analysis: &mut Analysis,
) {
    let left_value = resolve_expr(compilation, module, owner, left);
    let right_value = resolve_expr(compilation, module, owner, right);
    let combined = match (&left_value, &right_value) {
        (Some(left), Some(right)) => compose(op, left, right),
        _ => None,
    };
    if let (Some(left), Some(right)) = (&left_value, &right_value) {
        if combined.is_none() && left.is_complex() && right.is_complex() {
            analysis.diagnostics.push(
                Diagnostic::warning(
                    "composition-conflict",
                    format!(
                        "`{}` cannot compose {} and {}; neither the merge nor the concatenation rule applies",
                        op.symbol(),
                        left.kind(),
                        right.kind()
                    ),
                    path,
                    span,
                )
                .with_help(
                    "combine lists with lists, or dictionaries with dictionaries".to_string(),
                ),
            );
        }
    }
    let source = compilation.graph().get(module).source.as_str();
    analysis.compositions.push(Composition {
        module,
        span,
        enclosing,
        summary: describe_binary(
            compilation,
            source,
            op,
            left,
            right,
            left_value.as_ref(),
            right_value.as_ref(),
            combined.as_ref(),
        ),
        hint: format!("{} {}", op.symbol(), compose_word(op)),
    });
}

fn walk_list_merges(
    compilation: &Compilation,
    module: ModuleId,
    enclosing: Span,
    owner: Option<AnchorId>,
    block: &ast::Block,
    path: &Path,
    analysis: &mut Analysis,
) {
    let merges: Vec<&ast::MergeItem> = block
        .items
        .iter()
        .filter_map(|item| match item {
            BlockItem::Merge(merge) => Some(merge),
            _ => None,
        })
        .collect();
    if merges.is_empty() {
        return;
    }
    let source = compilation.graph().get(module).source.as_str();
    let mut steps: Vec<(String, Option<Value>, MergeOp)> = Vec::new();
    for item in &block.items {
        match item {
            BlockItem::ListItem(entry) => {
                let text = snippet(source, entry.span);
                let value = entry
                    .value
                    .inline
                    .as_ref()
                    .and_then(|line| resolve_line(compilation, module, owner, line))
                    .or_else(|| {
                        snippet(source, entry.value.span)
                            .is_empty()
                            .then_some(Value::Null)
                    });
                steps.push((text, value, MergeOp::Concat));
            }
            BlockItem::Merge(merge) => {
                let value = resolve_line(compilation, module, owner, &merge.value);
                steps.push((snippet(source, merge.span), value, merge.op));
            }
            _ => {}
        }
    }
    if steps.iter().all(|(_, value, _)| value.is_none()) {
        return;
    }
    let mut combined: Vec<Value> = Vec::new();
    let mut described = String::from("Composition");
    described.push_str("\n\n");
    for (index, (text, value, op)) in steps.iter().enumerate() {
        let word = match op {
            MergeOp::Merge => "merge, duplicates removed",
            MergeOp::Concat => "concatenate, duplicates kept",
        };
        described.push_str(&format!("{}. `{text}` ({word})", index + 1));
        if let Some(value) = value {
            described.push_str(&format!(" → {}", value.kind()));
            match op {
                MergeOp::Merge | MergeOp::Concat => {
                    let mut extra = match value {
                        Value::List(items) => items.clone(),
                        Value::Mixed(_) => value.as_list_items(),
                        other => vec![other.clone()],
                    };
                    combined.append(&mut extra);
                    if *op == MergeOp::Merge {
                        combined = dedup_keep_last(combined);
                    }
                }
            }
        }
        described.push_str("\n\n");
    }
    if !combined.is_empty() {
        described.push_str("Combined:\n\n```\n");
        described.push_str(&preview(compilation, &Value::List(combined)));
        described.push_str("\n```");
    }
    let span = merges
        .iter()
        .map(|merge| merge.span)
        .reduce(|left, right| left.cover(right))
        .unwrap_or(enclosing);
    let hint = if merges.iter().any(|merge| merge.op == MergeOp::Merge) {
        "+ merge"
    } else {
        "++ concat"
    };
    analysis.compositions.push(Composition {
        module,
        span,
        enclosing,
        summary: described,
        hint: hint.to_string(),
    });

    for merge in merges {
        if merge.op != MergeOp::Merge {
            continue;
        }
        let Some(value) = resolve_line(compilation, module, owner, &merge.value) else {
            continue;
        };
        if !value.is_complex() {
            continue;
        }
        let items = match &value {
            Value::List(items) => items.clone(),
            Value::Mixed(_) => value.as_list_items(),
            Value::Dict(_) => continue,
            other => vec![other.clone()],
        };
        // A merge that only repeats values already written above it changes
        // nothing once duplicates are removed.
        let prior = steps
            .iter()
            .take_while(|(text, _, _)| text.as_str() != snippet(source, merge.span))
            .filter_map(|(_, value, _)| value.clone())
            .flat_map(|value| match value {
                Value::List(items) => items,
                other => vec![other],
            })
            .collect::<Vec<_>>();
        if !items.is_empty()
            && items
                .iter()
                .all(|item| prior.iter().any(|have| have == item))
        {
            analysis.diagnostics.push(
                Diagnostic::warning(
                    "redundant-definition",
                    "this merge repeats values already in the list and can be removed",
                    path,
                    merge.span,
                )
                .with_help(
                    "`+` drops duplicates, so this operand does not change the resolved list"
                        .to_string(),
                ),
            );
        }
    }
}

fn describe_binary(
    compilation: &Compilation,
    source: &str,
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
    left_value: Option<&Value>,
    right_value: Option<&Value>,
    combined: Option<&Value>,
) -> String {
    let word = match op {
        BinaryOp::Add => {
            "merge (`+`): lists and dictionaries combine, duplicates removed, last kept"
        }
        BinaryOp::Concat => "concatenation (`++`): lists and dictionaries combine, duplicates kept",
        _ => "composition",
    };
    let mut out = format!("Composition — {word}\n\n");
    out.push_str(&format!("1. `{}`", snippet(source, left.span)));
    if let Some(value) = left_value {
        out.push_str(&format!(" → {}", value.kind()));
    }
    out.push_str(&format!("\n2. `{}`", snippet(source, right.span)));
    if let Some(value) = right_value {
        out.push_str(&format!(" → {}", value.kind()));
    }
    out.push_str("\n\n");
    match combined {
        Some(value) => {
            out.push_str("Combined:\n\n```\n");
            out.push_str(&preview(compilation, value));
            out.push_str("\n```");
        }
        None => {
            out.push_str("_These inputs do not combine under the operator's composition rule._")
        }
    }
    // Belay has no composition operator of its own: a skill list, an agent
    // prompt, and a shared instruction are Piton values combined with these
    // same operators, then inherited. Showing the inputs is the resolution.
    out
}

fn compose_word(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "merge",
        BinaryOp::Concat => "concat",
        _ => "compose",
    }
}

fn compose(op: BinaryOp, left: &Value, right: &Value) -> Option<Value> {
    let lists = matches!(
        (left.kind(), right.kind()),
        (ValueKind::List, ValueKind::List)
    );
    let dicts = matches!(
        (left.kind(), right.kind()),
        (ValueKind::Dictionary, ValueKind::Dictionary)
    );
    if !lists && !dicts {
        return None;
    }
    if lists {
        let mut items = left.as_list_items();
        items.extend(right.as_list_items());
        if op == BinaryOp::Add {
            items = dedup_keep_last(items);
        }
        return Some(Value::List(items));
    }
    let Value::Dict(left_map) = left else {
        return None;
    };
    let Value::Dict(right_map) = right else {
        return None;
    };
    let mut merged = left_map.clone();
    for (key, value) in right_map {
        merged.insert(key.clone(), value.clone());
    }
    Some(Value::Dict(merged))
}

fn resolve_line(
    compilation: &Compilation,
    module: ModuleId,
    owner: Option<AnchorId>,
    line: &ast::ProseLine,
) -> Option<Value> {
    if let Some(interpolation) = line.sole_interpolation() {
        return resolve_expr(compilation, module, owner, &interpolation.expr);
    }
    None
}

fn resolve_expr(
    compilation: &Compilation,
    module: ModuleId,
    owner: Option<AnchorId>,
    expr: &Expr,
) -> Option<Value> {
    match &expr.kind {
        ExprKind::Number(number) => Some(Value::Number(*number)),
        ExprKind::Bool(value) => Some(Value::Bool(*value)),
        ExprKind::Null => Some(Value::Null),
        ExprKind::Quoted(text) => Some(Value::string(text.clone())),
        ExprKind::List(items) => {
            let mut out = Vec::new();
            for item in items {
                out.push(resolve_expr(compilation, module, owner, item)?);
            }
            Some(Value::List(out))
        }
        ExprKind::Paren(inner) => resolve_expr(compilation, module, owner, inner),
        ExprKind::Name(name) => match compilation.resolution.lookup(module, name)? {
            Symbol::Anchor(anchor) => Some(Value::Anchor(anchor)),
            Symbol::Variable(variable) => compilation.store().variable(variable).value.clone(),
        },
        ExprKind::Field(base, field) => {
            let base = resolve_expr(compilation, module, owner, base)?;
            base.property(&field.value, compilation).cloned()
        }
        ExprKind::This | ExprKind::SelfRef => owner.map(Value::Anchor),
        ExprKind::Binary(op, left, right) => {
            let left = resolve_expr(compilation, module, owner, left)?;
            let right = resolve_expr(compilation, module, owner, right)?;
            compose(*op, &left, &right)
        }
        _ => None,
    }
}

fn dedup_keep_last(items: Vec<Value>) -> Vec<Value> {
    let mut keep = vec![true; items.len()];
    let mut seen: Vec<&Value> = Vec::new();
    for index in (0..items.len()).rev() {
        if seen.iter().any(|value| *value == &items[index]) {
            keep[index] = false;
        } else {
            seen.push(&items[index]);
        }
    }
    items
        .into_iter()
        .enumerate()
        .filter(|(index, _)| keep[*index])
        .map(|(_, value)| value)
        .collect()
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
// Source to output
// ---------------------------------------------------------------------------

fn map_outputs(compilation: &Compilation, mappings: &mut Vec<Mapping>) {
    for module in compilation.graph().iter() {
        if module.is_package() {
            continue;
        }
        let declarations = file_declarations(compilation, module.id);
        if declarations.is_empty() {
            continue;
        }
        let directory = module.path.parent().unwrap_or(Path::new("."));
        let context = piton_emit::MarkdownContext {
            from_directory: directory,
            source_root: &compilation.project.source_root,
        };
        for adapter in piton_emit::Adapter::ALL {
            let rendered = piton_emit::render(adapter, &declarations, compilation, context);
            let output = module.path.with_extension(adapter.extension());
            match adapter {
                piton_emit::Adapter::Markdown => {
                    map_markdown(
                        compilation,
                        &module.path,
                        &output,
                        &rendered,
                        &declarations,
                        mappings,
                    );
                }
                piton_emit::Adapter::Json | piton_emit::Adapter::Yaml => {
                    map_structured(
                        compilation,
                        &module.path,
                        &output,
                        adapter.as_str(),
                        &rendered,
                        &declarations,
                        mappings,
                    );
                }
            }
        }
    }
}

/// What a file compiles to, which is what the mapping has to describe: the
/// same surface `piton compile` renders, exports included.
fn file_declarations(compilation: &Compilation, module: ModuleId) -> piton_core::Properties {
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
    let end = rendered
        .find(markdown::LINK_FOOTER)
        .unwrap_or(rendered.len());
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
