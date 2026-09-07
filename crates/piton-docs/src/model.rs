//! Reading a framework's own module source to describe what it contributes.
//!
//! Frameworks describe themselves in Piton, so the documentation is extracted
//! by compiling their modules rather than by hand-maintaining a second list.

use piton_core::framework::Frameworks;
use piton_core::hir::{AnchorDef, Node, TypeExpr};
use piton_core::lower::lower;

/// One anchor a framework exports.
pub struct AnchorDoc {
    pub name: String,
    pub keyword: Option<String>,
    pub is_abstract: bool,
    pub doc: Option<String>,
    /// Property name and its declared constraint, when it has one.
    pub properties: Vec<(String, Option<String>)>,
}

/// One variable a framework exports.
pub struct VarDoc {
    pub name: String,
    pub doc: Option<String>,
}

/// Everything one framework contributes.
pub struct FrameworkDoc {
    pub name: String,
    pub module: String,
    pub sigils: Vec<String>,
    pub anchors: Vec<AnchorDoc>,
    pub vars: Vec<VarDoc>,
}

impl FrameworkDoc {
    /// The keywords this framework makes available through `use`.
    pub fn keywords(&self) -> Vec<String> {
        self.anchors.iter().filter_map(|anchor| anchor.keyword.clone()).collect()
    }
}

/// Describe every registered framework by reading its modules.
pub fn describe(frameworks: &Frameworks) -> Vec<FrameworkDoc> {
    let mut docs = Vec::new();
    for framework in &frameworks.active {
        for module in framework.modules() {
            let parse = piton_syntax::parse(&module.source);
            let hir = lower(&parse.root());
            docs.push(FrameworkDoc {
                name: framework.name().to_string(),
                module: module.name.clone(),
                sigils: framework.sigils(),
                anchors: hir
                    .anchors
                    .iter()
                    .filter(|anchor| anchor.exported)
                    .map(describe_anchor)
                    .collect(),
                vars: hir
                    .vars
                    .iter()
                    .filter(|var| var.exported)
                    .map(|var| VarDoc { name: var.name.clone(), doc: var.doc.clone() })
                    .collect(),
            });
        }
    }
    docs
}

fn describe_anchor(anchor: &AnchorDef) -> AnchorDoc {
    let mut properties = Vec::new();
    if let Node::Dict(props) = &anchor.body {
        for property in props {
            properties.push((property.name.clone(), render_constraints(&property.constraints)));
        }
    }
    AnchorDoc {
        name: anchor.name.clone(),
        keyword: anchor.keyword.as_ref().map(|it| it.value.clone()),
        is_abstract: anchor.is_abstract,
        doc: anchor.doc.clone(),
        properties,
    }
}

fn render_constraints(constraints: &[TypeExpr]) -> Option<String> {
    if constraints.is_empty() {
        return None;
    }
    Some(constraints.iter().map(TypeExpr::render).collect::<Vec<_>>().join(":: "))
}
