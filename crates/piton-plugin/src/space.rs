//! Link spaces and encoders. These are the renderers of the compiler, and
//! plugins can use them.
//!
//! A plugin that writes documents tells the location of each anchor. The host
//! then renders the documents. References become links to the header of
//! their target. The links are relative to the output file. They use the
//! fragments that a Markdown viewer gives to repeated headings. Belay writes
//! its reference tree with the same code. Thus the documents of a plugin
//! agree with the documents of the compiler.

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

/// One anchor in a document. It also holds the properties to omit when they
/// are null.
#[derive(Debug, Clone)]
struct Part {
    anchor: AnchorId,
    omit_null: Vec<String>,
}

/// The location of each anchor in the documents of a plugin, and the
/// headings of each document.
pub struct Space {
    /// The handle of the compilation that it renders.
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
    /// Makes the layout of the documents of a plugin. `documents` lists each
    /// output path with its anchors in order. Each anchor is `{id,
    /// omitNull}`. `files` lists the anchors that each go to a separate file
    /// with no headings. Each entry is `{id, path}`. Paths are relative to
    /// the project root.
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
        // Headings do not depend on links. Thus a first render finds the
        // headings of all documents. Later renders make links to them.
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

    /// `{from, id, omitNull}`. Gives the document of an anchor, with links
    /// relative to `from`.
    pub fn document(&self, compilation: &Compilation, payload: &Json) -> Result<Json, String> {
        let from = PathBuf::from(payload.get("from").and_then(Json::as_str).ok_or("`from` is the file being written")?);
        let part = Part {
            anchor: anchor_id(compilation, payload.get("id").unwrap_or(&Json::Null))?,
            omit_null: strings(payload.get("omitNull").unwrap_or(&Json::Null)),
        };
        Ok(json!(self.render(compilation, &from, &part, true)))
    }

    /// `{from, id, keys, level}`. Gives some properties of an anchor as
    /// sections, in the order of the anchor. The first level is `level`. It
    /// is empty when the anchor has none of the properties.
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

    /// `{from, id, key}`. Gives a text property as Markdown, with references
    /// as links. It is null when the property is not text.
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

    /// The results of the render. It gives the references to anchors that no
    /// file holds. It also gives each written link and the directory of that
    /// link. Thus a plugin can check its links. Both lists have a stable
    /// order.
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

/// The location of each anchor in a link space, as links relative to the
/// project. Each location is a file. When the file is a document, the
/// location also has the header of the reference. A command uses this to
/// cite compiled output and not the source.
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

/// Converts plugin JSON into typed fields. The permitted types are text,
/// booleans, numbers, lists of text, and nested dictionaries. Metadata cannot
/// show other types.
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

/// `{format, value}`. Encodes with the encoders of the compiler.
/// `frontmatter`, `yaml` and `toml` take a dictionary of fields.
/// `tomlString`, `yamlScalar` and `jsonString` take text.
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
