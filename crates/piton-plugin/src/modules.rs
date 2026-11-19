//! Modules that plugins give for files that are not Piton.
//!
//! A plugin can say `modules: [emul]` in its declaration. A `.pi` file can
//! then import a `.emul` file. When the compiler loads such a file, it calls
//! the module hook of the plugin with the path and the text of the file. The
//! hook gives the Piton source of the module. The compiler then treats it as
//! all other modules. The compiler knows only the extensions. All knowledge
//! of the file format is in the plugin.
//!
//! The provider keeps each result by path and text. Thus the hook runs again
//! only when the text of the file changes.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use piton_compile::module::{set_module_provider, ModuleProvider};
use piton_compile::{Compilation, Project};
use piton_core::Diagnostic;
use serde_json::Value as Json;

use crate::host::{run_module, Services, Stream};
use crate::manifest::Manifest;

thread_local! {
    /// True while a module hook runs on this thread. A hook that compiles the
    /// project then receives no modules from plugins. Thus a hook cannot call
    /// itself without end.
    static IN_HOOK: Cell<bool> = const { Cell::new(false) };
}

/// The provider that runs the module hooks of the plugins of one project.
pub struct Provider {
    project: Project,
    manifests: Vec<Manifest>,
    cache: Mutex<HashMap<(PathBuf, String), Result<String, String>>>,
}

impl Provider {
    fn owner(&self, extension: &str) -> Option<&Manifest> {
        self.manifests
            .iter()
            .find(|manifest| manifest.modules.iter().any(|known| known == extension))
    }
}

impl ModuleProvider for Provider {
    fn provides(&self, extension: &str) -> bool {
        self.owner(extension).is_some()
    }

    fn module(&self, path: &Path, text: &str) -> Result<String, String> {
        let key = (path.to_path_buf(), text.to_string());
        if let Some(found) = self.cache.lock().unwrap_or_else(|error| error.into_inner()).get(&key) {
            return found.clone();
        }
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
        let Some(manifest) = self.owner(extension) else {
            return Err(format!("no plugin gives modules for `.{extension}` files"));
        };
        if IN_HOOK.with(Cell::get) {
            return Err("a module hook cannot compile a project that imports such a file".to_string());
        }
        let relative = path
            .strip_prefix(&self.project.root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| path.to_path_buf());
        let relative = relative.to_string_lossy().replace('\\', "/");
        IN_HOOK.with(|flag| flag.set(true));
        let services: Rc<dyn Services> = Rc::new(Quiet { project: self.project.clone() });
        let result = run_module(&self.manifests, &manifest.name, &relative, text, services);
        IN_HOOK.with(|flag| flag.set(false));
        let result = result
            .map_err(|failure| format!("plugin `{}` failed: {failure}", manifest.name))
            .and_then(|value| read(&manifest.name, &value));
        self.cache
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(key, result.clone());
        result
    }
}

/// Reads the result of a module hook: the Piton source as a string, or
/// `{source}`.
fn read(plugin: &str, value: &Json) -> Result<String, String> {
    value
        .as_str()
        .or_else(|| value.get("source").and_then(Json::as_str))
        .map(str::to_string)
        .ok_or_else(|| format!("the module hook of plugin `{plugin}` gave no Piton source"))
}

/// Makes the module hooks of `manifests` the provider of modules for the
/// compiler. When no plugin declares a module hook, the compiler loads only
/// Piton files.
pub fn install(project: &Project, manifests: &[Manifest]) {
    if manifests.iter().all(|manifest| manifest.modules.is_empty()) {
        set_module_provider(None);
        return;
    }
    set_module_provider(Some(Arc::new(Provider {
        project: project.clone(),
        manifests: manifests.to_vec(),
        cache: Mutex::new(HashMap::new()),
    })));
}

/// The services of a module hook. A hook gives a module. It prints nothing.
struct Quiet {
    project: Project,
}

impl Services for Quiet {
    fn project(&self) -> &Project {
        &self.project
    }
    fn write(&self, _: Stream, _: &str) {}
    fn paint(&self, _: &str, text: &str) -> String {
        text.to_string()
    }
    fn report(&self, _: &str, _: &str) {}
    fn diagnostic(&self, _: Diagnostic) {}
    fn compilation_diagnostics(&self, _: &Compilation) -> bool {
        false
    }
    fn slice(&self, _: &Compilation, _: &str, _: &dyn piton_emit::markdown::LinkResolver) -> u8 {
        1
    }
}
