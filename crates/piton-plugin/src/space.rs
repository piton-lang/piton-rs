//! Link spaces and encoders: the compiler's own renderers, offered to
//! plugins.
//!
//! A plugin that writes documents says where each anchor goes, and the host
//! renders them: references become links to the heading their target lands
//! on, relative to the file being written, with the fragments a Markdown
//! viewer gives repeated headings. This is the same code Belay's reference
//! tree is written with, so a plugin's documents match the compiler's.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use piton_compile::Compilation;
use piton_core::{AnchorId, AnchorView, Properties, Ref, Value};
use piton_emit::fields::{self, FieldValue, Fields};
use piton_emit::links::{FileIndex, Links};
use piton_emit::markdown;
use serde_json::{json, Value as Json};

/// One anchor in a document, with the properties to leave out when they are
/// null.
#[derive(Debug, Clone)]
struct Part {
    anchor: AnchorId,
    omit_null: Vec<String>,
}

/// Where a plugin's documents put each anchor, and the headings each
/// document has.
pub struct Space {
    /// The compilation it renders, by handle.
    pub compilation: usize,
    locations: HashMap<AnchorId, PathBuf>,
    documents: BTreeMap<PathBuf, Vec<Part>>,
    index: HashMap<PathBuf, FileIndex>,
    misses: RefCell<Vec<Ref>>,
    generated: RefCell<HashSet<(PathBuf, String)>>,
}

fn anchor_id(compilation: &Compilation, value: &Json) -> Result<AnchorId, String> {
    let id = value.as_u64().ok_or("an anchor is given by its id")?;
    if id as usize >= compilation.store().anchors.len() {
        return Err(format!("no anchor has the id {id}"));
    }
    Ok(AnchorId(id as u32))
}

fn strings(value: &Json) -> Vec<String> {
    value
        .as_array()
        .map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

impl Space {
    /// Lays out a plugin's documents. `documents` lists each output path with
    /// the anchors it holds, in order, each `{id, omitNull}`; `files` lists
    /// anchors written to a file of their own, without headings, `{id,
    /// path}`. Paths are relative to the project root.
    pub fn new(handle: usize, compilation: &Compilation, payload: &Json) -> Result<Space, String> {
        let mut locations = HashMap::new();
        let mut documents: BTreeMap<PathBuf, Vec<Part>> = BTreeMap::new();
        for document in payload.get("documents").and_then(Json::as_array).into_iter().flatten() {
            let path = PathBuf::from(document.get("path").and_then(Json::as_str).ok_or("a document needs a path")?);
            for part in document.get("anchors").and_then(Json::as_array).into_iter().flatten() {
                let anchor = anchor_id(compilation, part.get("id").unwrap_or(part))?;
                locations.insert(anchor, path.clone());
                documents.entry(path.clone()).or_default().push(Part {
                    anchor,
                    omit_null: strings(part.get("omitNull").unwrap_or(&Json::Null)),
                });
            }
        }
        for file in payload.get("files").and_then(Json::as_array).into_iter().flatten() {
            let anchor = anchor_id(compilation, file.get("id").unwrap_or(&Json::Null))?;
            let path = file.get("path").and_then(Json::as_str).ok_or("a file needs a path")?;
            locations.insert(anchor, PathBuf::from(path));
        }
        let mut space = Space {
            compilation: handle,
            locations,
            documents,
            index: HashMap::new(),
            misses: RefCell::new(Vec::new()),
            generated: RefCell::new(HashSet::new()),
        };
        // Headings don't depend on links, so a first rendering finds every
        // document's headings, and later renderings link to them.
        let mut index = HashMap::new();
        for (path, parts) in &space.documents {
            let rendered: Vec<(AnchorId, String)> = parts
                .iter()
                .map(|part| (part.anchor, space.render(compilation, path, part, false)))
                .collect();
            let borrowed: Vec<(AnchorId, &str)> = rendered.iter().map(|(a, t)| (*a, t.as_str())).collect();
            index.insert(path.clone(), FileIndex::build(&borrowed));
        }
        space.index = index;
        space.misses.borrow_mut().clear();
        space.generated.borrow_mut().clear();
        Ok(space)
    }

    fn links<'a>(&'a self, compilation: &'a Compilation, from: &Path, indexed: bool) -> Links<'a> {
        Links {
            from_directory: from.parent().unwrap_or(Path::new("")).to_path_buf(),
            anchors: compilation,
            locations: &self.locations,
            index: indexed.then_some(&self.index),
            misses: &self.misses,
            generated: &self.generated,
        }
    }

    fn render(&self, compilation: &Compilation, from: &Path, part: &Part, indexed: bool) -> String {
        let links = self.links(compilation, from, indexed);
        let context = markdown::Context { anchors: compilation, links: &links };
        if part.omit_null.is_empty() {
            return markdown::document(part.anchor, &context);
        }
        let mut properties = compilation.properties(part.anchor).clone();
        for field in &part.omit_null {
            if matches!(properties.get(field), Some(Value::Null)) {
                properties.shift_remove(field);
            }
        }
        markdown::document_with(part.anchor, &properties, &context)
    }

    /// `{from, id, omitNull}`: an anchor's document, with links relative to
    /// `from`.
    pub fn document(&self, compilation: &Compilation, payload: &Json) -> Result<Json, String> {
        let from = PathBuf::from(payload.get("from").and_then(Json::as_str).ok_or("`from` is the file being written")?);
        let part = Part {
            anchor: anchor_id(compilation, payload.get("id").unwrap_or(&Json::Null))?,
            omit_null: strings(payload.get("omitNull").unwrap_or(&Json::Null)),
        };
        Ok(json!(self.render(compilation, &from, &part, true)))
    }

    /// `{from, id, keys, level}`: some of an anchor's properties as sections,
    /// in the anchor's order, starting at `level`. Empty when none of them is
    /// there.
    pub fn sections(&self, compilation: &Compilation, payload: &Json) -> Result<Json, String> {
        let from = PathBuf::from(payload.get("from").and_then(Json::as_str).ok_or("`from` is the file being written")?);
        let anchor = anchor_id(compilation, payload.get("id").unwrap_or(&Json::Null))?;
        let keys = strings(payload.get("keys").unwrap_or(&Json::Null));
        let level = payload.get("level").and_then(Json::as_u64).unwrap_or(1) as usize;
        let properties: Properties = compilation
            .properties(anchor)
            .iter()
            .filter(|(key, _)| keys.contains(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if properties.is_empty() {
            return Ok(json!(""));
        }
        let links = self.links(compilation, &from, true);
        let context = markdown::Context { anchors: compilation, links: &links };
        Ok(json!(markdown::sections(&properties, level, &context)))
    }

    /// `{from, id, key}`: a text property as Markdown, references as links;
    /// null when the property isn't text.
    pub fn text(&self, compilation: &Compilation, payload: &Json) -> Result<Json, String> {
        let from = PathBuf::from(payload.get("from").and_then(Json::as_str).ok_or("`from` is the file being written")?);
        let anchor = anchor_id(compilation, payload.get("id").unwrap_or(&Json::Null))?;
        let key = payload.get("key").and_then(Json::as_str).ok_or("`key` is the property")?;
        let links = self.links(compilation, &from, true);
        let context = markdown::Context { anchors: compilation, links: &links };
        Ok(match compilation.properties(anchor).get(key) {
            Some(Value::Str(text)) => json!(markdown::inline_text(text, &context)),
            _ => Json::Null,
        })
    }

    /// What rendering found: references to anchors nothing wrote, and every
    /// link written with the directory it is relative to, so a plugin can
    /// check the links it wrote. Both in a stable order.
    pub fn report(&self, compilation: &Compilation) -> Json {
        let mut misses: Vec<Json> = self
            .misses
            .borrow()
            .iter()
            .map(|miss| {
                json!({
                    "id": miss.anchor.0,
                    "anchor": compilation.name(miss.anchor),
                    "path": miss.path,
                })
            })
            .collect();
        misses.sort_by_key(|miss| miss.to_string());
        misses.dedup();
        let mut links: Vec<(String, String)> = self
            .generated
            .borrow()
            .iter()
            .map(|(from, link)| (crate::output::slash(from), link.clone()))
            .collect();
        links.sort();
        json!({
            "misses": misses,
            "links": links.into_iter().map(|(from, link)| json!({ "from": from, "link": link })).collect::<Vec<_>>(),
        })
    }
}

/// Where a link space put each anchor, as project-relative links: the file,
/// and the heading a reference lands on when the file is a document. This is
/// how a command cites compiled output instead of the source.
pub struct Located<'a> {
    pub space: &'a Space,
    pub compilation: &'a Compilation,
}

impl markdown::LinkResolver for Located<'_> {
    fn link(&self, target: &Ref) -> Option<String> {
        let location = self.space.locations.get(&target.anchor)?;
        let file = crate::output::slash(location);
        match self.space.index.get(location) {
            Some(headings) => {
                let fragment = headings
                    .fragment(target, self.compilation)
                    .unwrap_or_else(|| piton_emit::reference_fragment(self.compilation, target));
                Some(format!("{file}#{fragment}"))
            }
            None => Some(file),
        }
    }
}

/// Converts plugin JSON into typed fields: text, booleans, numbers, lists of
/// text, and nested dictionaries. Anything else has no representation in
/// metadata.
fn field_value(value: &Json) -> Result<FieldValue, String> {
    Ok(match value {
        Json::String(text) => FieldValue::Text(text.clone()),
        Json::Bool(flag) => FieldValue::Bool(*flag),
        Json::Number(number) => FieldValue::Number(number.as_f64().unwrap_or(0.0)),
        Json::Array(items) => FieldValue::List(
            items
                .iter()
                .map(|item| item.as_str().map(str::to_string).ok_or("a list field holds text only"))
                .collect::<Result<_, _>>()?,
        ),
        Json::Object(map) => {
            let mut out = IndexMap::new();
            for (key, item) in map {
                out.insert(key.clone(), field_value(item)?);
            }
            FieldValue::Map(out)
        }
        Json::Null => return Err("a field can't be null; leave it out".to_string()),
    })
}

fn fields_of(value: &Json) -> Result<Fields, String> {
    let map = value.as_object().ok_or("fields are a dictionary")?;
    let mut out = Fields::new();
    for (key, item) in map {
        out.insert(key.clone(), field_value(item)?);
    }
    Ok(out)
}

/// `{format, value}`: encodes with the compiler's encoders. `frontmatter`,
/// `yaml` and `toml` take a dictionary of fields; `tomlString`, `yamlScalar`
/// and `jsonString` take text.
pub fn encode(payload: &Json) -> Result<Json, String> {
    let value = payload.get("value").unwrap_or(&Json::Null);
    let text = || value.as_str().map(str::to_string).ok_or("this format encodes text");
    Ok(json!(match payload.get("format").and_then(Json::as_str) {
        Some("frontmatter") => fields::frontmatter(&fields_of(value)?),
        Some("yaml") => fields::yaml_document(&fields_of(value)?),
        Some("toml") => fields::toml_document(&fields_of(value)?),
        Some("tomlString") => fields::toml_string(&text()?),
        Some("yamlScalar") => fields::yaml_scalar(&text()?),
        Some("jsonString") => Json::String(text()?).to_string(),
        _ => return Err("format is frontmatter, yaml, toml, tomlString, yamlScalar or jsonString".to_string()),
    }))
}
