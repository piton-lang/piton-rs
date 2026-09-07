//! Belay's configuration, read out of the project's `belay-config` anchor.

use std::path::{Path, PathBuf};

use piton_core::db::canonical;
use piton_core::value::Value;

/// One output target, such as `.claude`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Adapter {
    /// The agent directory, relative to the project base.
    pub directory: PathBuf,
    /// The tool this directory belongs to, for diagnostics.
    pub label: String,
}

/// The tools an agent adapter knows how to write for.
const KNOWN_TOOLS: &[(&str, &str)] = &[
    ("claude", ".claude"),
    ("opencode", ".opencode"),
    ("codex", ".codex"),
    ("cursor", ".cursor"),
    ("agents", ".agents"),
];

/// Everything Belay needs to emit.
#[derive(Clone, Debug, Default)]
pub struct Settings {
    /// The directory holding the project's source code.
    pub code_root: PathBuf,
    /// The directory whose structure mirrors `code_root`.
    pub shape_root: Option<PathBuf>,
    /// Where output goes, one entry per configured adapter.
    pub adapters: Vec<Adapter>,
    /// The project base, so relative output paths can be produced.
    pub base: PathBuf,
    /// The project source root, used when a reference is not under the shape.
    pub source_root: PathBuf,
}

impl Settings {
    /// Read a `belay-config` anchor.
    pub fn from_config(base: &Path, source_root: &Path, config: &Value) -> (Settings, Vec<String>) {
        let mut messages = Vec::new();
        let mut settings = Settings {
            code_root: canonical(base),
            base: canonical(base),
            source_root: canonical(source_root),
            ..Settings::default()
        };
        match config.field("codeRoot") {
            Some(Value::Str(path)) => settings.code_root = canonical(&base.join(path)),
            _ => messages.push("belay-config needs a `codeRoot` string".to_string()),
        }
        if let Some(Value::Str(path)) = config.field("shapeRoot") {
            settings.shape_root = Some(canonical(&base.join(path)));
        }
        if let Some(Value::List(list)) = config.field("adapters") {
            for adapter in &list.items {
                settings.adapters.extend(read_adapter(adapter, &mut messages));
            }
        }
        if settings.adapters.is_empty() {
            messages.push(
                "belay-config lists no adapters; defaulting to the Claude adapter".to_string(),
            );
            settings.adapters.push(Adapter {
                directory: PathBuf::from(".claude"),
                label: "claude".to_string(),
            });
        }
        (settings, messages)
    }

    /// The adapter whose paths appear in `@{}` references.
    pub fn primary(&self) -> &Adapter {
        &self.adapters[0]
    }

    /// The compiled shape directory, relative to the project base.
    pub fn shape_reference_dir(&self) -> PathBuf {
        self.primary().directory.join("reference").join("shape")
    }
}

/// Read one adapter anchor into a target directory.
fn read_adapter(value: &Value, messages: &mut Vec<String>) -> Vec<Adapter> {
    if let Some(Value::Str(directory)) = value.field("directory") {
        return vec![Adapter { directory: PathBuf::from(directory), label: directory.clone() }];
    }
    let mut adapters = Vec::new();
    for (flag, directory) in KNOWN_TOOLS {
        if matches!(value.field(flag), Some(Value::Bool(true))) {
            adapters.push(Adapter {
                directory: PathBuf::from(directory),
                label: (*flag).to_string(),
            });
        }
    }
    if adapters.is_empty() {
        messages.push(format!(
            "adapter `{}` enables no known tool; set one of {} to true, or set `directory`",
            match value {
                Value::Anchor(anchor) => anchor.name.as_str(),
                other => other.type_name(),
            },
            KNOWN_TOOLS.iter().map(|(flag, _)| *flag).collect::<Vec<_>>().join(", ")
        ));
    }
    adapters
}

/// Where the compiled Markdown for an anchor lives inside an agent directory.
///
/// Anchors under the shape root keep their structure beneath `reference/shape`;
/// anything else lands under `reference`.
pub fn reference_path(settings: &Settings, adapter: &Adapter, source: &Path) -> PathBuf {
    let under_shape = settings
        .shape_root
        .as_ref()
        .and_then(|root| source.strip_prefix(root).ok())
        .map(|relative| (true, relative.to_path_buf()));
    let relative = under_shape.or_else(|| {
        source.strip_prefix(&settings.source_root).ok().map(|it| (false, it.to_path_buf()))
    });
    let (shape, relative) = relative.unwrap_or_else(|| {
        (false, PathBuf::from(source.file_name().unwrap_or_default()))
    });
    let mut path = adapter.directory.join("reference");
    if shape {
        path.push("shape");
    }
    path.push(relative);
    path.set_extension("md");
    path
}
