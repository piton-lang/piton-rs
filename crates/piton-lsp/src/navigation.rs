//! Resolving what is under the cursor, and everything that follows from it.

use piton_core::hir::Node;
use piton_core::resolve::Symbol;
use piton_core::types;
use piton_core::value::AnchorId;
use piton_core::FileId;
use piton_syntax::ast::{self, AstNode};
use piton_syntax::kind::SyntaxKind::{self, *};
use piton_syntax::{SyntaxNode, SyntaxToken, TextRange, TextSize};

use crate::world::Snapshot;

/// What a token under the cursor refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A named anchor or variable.
    Symbol(Symbol),
    /// A user-defined keyword and the anchor it stands for.
    Keyword(AnchorId),
    /// An imported module.
    Module(FileId),
    /// A built-in type name.
    Builtin(String),
    /// A property, with the anchor that owns it when it is known.
    Property { owner: Option<AnchorId>, name: String },
}

/// A resolved token.
#[derive(Clone, Debug)]
pub struct Resolved {
    pub range: TextRange,
    pub text: String,
    pub target: Target,
}

/// The meaningful token at an offset.
///
/// An offset between two tokens belongs to both; the token to the right wins,
/// because that is the one the cursor is sitting in front of.
pub fn token_at(root: &SyntaxNode, offset: TextSize) -> Option<SyntaxToken> {
    let candidates: Vec<SyntaxToken> = root.token_at_offset(offset).collect();
    candidates
        .iter()
        .rev()
        .find(|token| carries_meaning(token.kind()))
        .cloned()
        .or_else(|| candidates.into_iter().next())
}

fn carries_meaning(kind: SyntaxKind) -> bool {
    !kind.is_trivia() && !matches!(kind, NEWLINE | BLANK)
}

/// Work out what the token at `offset` refers to.
pub fn resolve(snapshot: &Snapshot, file: FileId, offset: TextSize) -> Option<Resolved> {
    let root = snapshot.compilation.analysis.db.file(file).parse.syntax();
    let token = token_at(&root, offset)?;
    let parent = token.parent()?;
    let text = token.text().to_string();
    let range = token.text_range();
    let analysis = &snapshot.compilation.analysis;
    let scope = analysis.scope(file);

    let target = match (token.kind(), parent.kind()) {
        (PATH, IMPORT_DECL | REEXPORT_DECL | USE_DECL) => {
            Target::Module(analysis.module(file, &text)?)
        }
        (IDENT, IMPORT_ITEM) => {
            let declaration = parent.ancestors().find(|node| {
                matches!(node.kind(), IMPORT_DECL | REEXPORT_DECL)
            })?;
            let path = declaration
                .children_with_tokens()
                .filter_map(|it| it.into_token())
                .find(|it| it.kind() == PATH)?;
            let module = analysis.module(file, path.text())?;
            let item = ast::ImportItem::cast(parent.clone())?;
            // An alias points at the same export as the name it renames.
            let name = item.name()?;
            Target::Symbol(*analysis.scope(module).exports.get(&name)?)
        }
        (IDENT, EXTENDS_CLAUSE) => Target::Symbol(*scope.names.get(&text)?),
        (IDENT, ANCHOR_DECL) => {
            let declaration = ast::AnchorDecl::cast(parent.clone())?;
            if declaration.keyword_token().map(|it| it.text_range()) == Some(range) {
                Target::Keyword(*scope.keywords.get(&text)?)
            } else {
                let index = anchor_index(snapshot, file, &parent)?;
                Target::Symbol(Symbol::Anchor(analysis.anchor_id(file, index)?))
            }
        }
        (IDENT, AS_CLAUSE) => {
            let declaration = parent.parent()?;
            let index = anchor_index(snapshot, file, &declaration)?;
            Target::Keyword(analysis.anchor_id(file, index)?)
        }
        (IDENT | ANCHOR_KW | NULL_KW, TYPE_REF) => match scope.names.get(&text) {
            Some(symbol) => Target::Symbol(*symbol),
            None if types::is_builtin(&text) => Target::Builtin(text.clone()),
            None => return None,
        },
        (IDENT, NAME_REF) => Target::Symbol(*scope.names.get(&text)?),
        (THIS_KW | SELF_KW | SUPER_KW, NAME_REF) => {
            Target::Keyword(enclosing_anchor(snapshot, file, &parent)?)
        }
        (IDENT | THIS_KW | SELF_KW | SUPER_KW, FIELD_EXPR) => Target::Property {
            owner: enclosing_anchor(snapshot, file, &parent),
            name: text.clone(),
        },
        (IDENT, EXPORT_DECL) => Target::Symbol(*scope.names.get(&text)?),
        (IDENT, PROPERTY) => {
            Target::Property { owner: enclosing_anchor(snapshot, file, &parent), name: text.clone() }
        }
        (IDENT, VAR_DECL) => Target::Symbol(*scope.names.get(&text)?),
        _ => return None,
    };
    Some(Resolved { range, text, target })
}

/// Where a target was declared.
pub fn definition(snapshot: &Snapshot, target: &Target) -> Option<(FileId, TextRange)> {
    let analysis = &snapshot.compilation.analysis;
    match target {
        Target::Symbol(Symbol::Anchor(id)) | Target::Keyword(id) => {
            let location = analysis.anchor_loc(*id);
            Some((location.file, analysis.anchor_def(*id).name_range))
        }
        Target::Symbol(Symbol::Var { file, index }) => {
            Some((*file, analysis.db.file(*file).hir.vars[*index].name_range))
        }
        Target::Module(file) => Some((*file, TextRange::empty(0.into()))),
        Target::Builtin(_) => None,
        Target::Property { owner, name } => {
            let owner = (*owner)?;
            property_declaration(snapshot, owner, name)
        }
    }
}

/// Find where a property was written, following the inheritance chain.
pub fn property_declaration(
    snapshot: &Snapshot,
    anchor: AnchorId,
    name: &str,
) -> Option<(FileId, TextRange)> {
    let analysis = &snapshot.compilation.analysis;
    let mut chain = vec![anchor];
    chain.extend(analysis.ancestors(anchor));
    for link in chain {
        let location = analysis.anchor_loc(link);
        let mut properties = Vec::new();
        collect(&analysis.anchor_def(link).body, &mut properties);
        if let Some(property) = properties.iter().find(|property| property.name == name) {
            return Some((location.file, property.name_range));
        }
    }
    None
}

/// Every place a target is mentioned.
pub fn references(snapshot: &Snapshot, target: &Target) -> Vec<(FileId, TextRange)> {
    let mut out = Vec::new();
    let files: Vec<FileId> = snapshot.compilation.analysis.db.files().map(|it| it.id).collect();
    for file in files {
        let root = snapshot.compilation.analysis.db.file(file).parse.syntax();
        for token in root.descendants_with_tokens().filter_map(|it| it.into_token()) {
            if !matches!(token.kind(), IDENT | PATH | THIS_KW | SELF_KW | SUPER_KW) {
                continue;
            }
            let Some(resolved) = resolve(snapshot, file, token.text_range().start()) else {
                continue;
            };
            if resolved.target == *target && resolved.range == token.text_range() {
                out.push((file, token.text_range()));
            }
        }
    }
    out
}

/// Concrete anchors implementing an abstract one.
pub fn implementations(snapshot: &Snapshot, target: &Target) -> Vec<AnchorId> {
    match target {
        Target::Symbol(Symbol::Anchor(id)) | Target::Keyword(id) => {
            snapshot.compilation.analysis.implementors(*id)
        }
        _ => Vec::new(),
    }
}

/// The anchor an expression is written inside, if any.
pub fn enclosing_anchor(snapshot: &Snapshot, file: FileId, node: &SyntaxNode) -> Option<AnchorId> {
    let declaration = node.ancestors().find(|it| it.kind() == ANCHOR_DECL)?;
    let index = anchor_index(snapshot, file, &declaration)?;
    snapshot.compilation.analysis.anchor_id(file, index)
}

/// Which anchor in the file's HIR a declaration node corresponds to.
fn anchor_index(snapshot: &Snapshot, file: FileId, declaration: &SyntaxNode) -> Option<usize> {
    let range = declaration.text_range();
    snapshot
        .compilation
        .analysis
        .db
        .file(file)
        .hir
        .anchors
        .iter()
        .position(|anchor| anchor.range == range)
}

fn collect(node: &Node, out: &mut Vec<piton_core::hir::Property>) {
    match node {
        Node::Dict(properties) => out.extend(properties.iter().cloned()),
        Node::Mixed(nodes) => nodes.iter().for_each(|node| collect(node, out)),
        Node::List(elements) | Node::Merge(elements) => {
            elements.iter().for_each(|element| collect(&element.node, out))
        }
        _ => {}
    }
}

/// Every property visible on an anchor, nearest definition first.
pub fn visible_properties(snapshot: &Snapshot, anchor: AnchorId) -> Vec<piton_core::hir::Property> {
    let analysis = &snapshot.compilation.analysis;
    let mut chain = analysis.ancestors(anchor);
    chain.reverse();
    chain.push(anchor);
    let mut out: Vec<piton_core::hir::Property> = Vec::new();
    for link in chain {
        let mut properties = Vec::new();
        collect(&analysis.anchor_def(link).body, &mut properties);
        for mut property in properties {
            match out.iter_mut().find(|existing| existing.name == property.name) {
                Some(existing) => {
                    // A redefinition without a `::` keeps the inherited
                    // constraint, exactly as the evaluator resolves it.
                    if property.constraints.is_empty() {
                        property.constraints = existing.constraints.clone();
                    }
                    *existing = property;
                }
                None => out.push(property),
            }
        }
    }
    out
}

/// True when a token can be renamed: only identifiers ever can.
pub fn is_renameable(kind: SyntaxKind) -> bool {
    matches!(kind, IDENT)
}
