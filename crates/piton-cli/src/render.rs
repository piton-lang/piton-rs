//! The only place where the CLI renders a compiled module.
//!
//! `piton compile` and `piton build` both change the compiled surface of a
//! module into renderer output. The call to `piton_emit::render` is only here.
//! Thus a change to the render setup occurs in one place. An example is the
//! context that a renderer gets.

use std::path::Path;

use piton_compile::{prelude, Compilation, ModuleId};
use piton_emit::{Adapter, MarkdownContext};

/// Parses a renderer name in the form of the command line or the configuration.
pub fn parse_renderer(name: &str) -> Result<Adapter, String> {
    name.parse::<Adapter>().map_err(|_| {
        format!("unknown renderer `{name}`; valid options are json, yaml, markdown")
    })
}

/// Renders the compiled output of a module.
///
/// `output_directory` is the directory of the rendered file. `--write` or a
/// build puts the file there. For stdout, it is the directory of the source
/// file. The links that a renderer writes are relative to this directory.
/// `output_root` is the start of the output tree when the tree is not next to
/// the sources, as with a build. Thus a reference goes to the output of the
/// other file.
pub fn render_module(
    compilation: &Compilation,
    module: ModuleId,
    renderer: Adapter,
    output_directory: &Path,
    output_root: Option<&Path>,
) -> String {
    let surface = compilation.compiled_surface(module);
    let rendered = piton_emit::render_mapped(
        renderer,
        &surface,
        compilation,
        MarkdownContext {
            from_directory: output_directory,
            source_root: &compilation.project.source_root,
        },
        output_root,
    );
    resolve_locations(compilation, &rendered, output_directory)
}

/// Gives values to the special imports of Belay for plain renderer output.
///
/// They are paths relative to the file that contains them. The project, code,
/// and shape roots need no adapter. The agent directory and its
/// compiled shape exist only for an adapter. Thus, without an adapter, the
/// output shows their names, not the internal markers of the compiler.
fn resolve_locations(compilation: &Compilation, text: &str, output_directory: &Path) -> String {
    if !text.contains('\u{e000}') {
        return text.to_string();
    }
    let project = &compilation.project;
    let relative = |target: &Path| {
        piton_emit::relative_file(output_directory, &target.join("x"))
            .trim_end_matches("/x")
            .to_string()
    };
    let project_root = relative(&project.root);
    // The code and shape roots are settings of Belay. Only the output of Belay
    // resolves them. The renderer writes them as the project root.
    let code_root = project_root.clone();
    let shape_root = project_root.clone();
    let mut out = text.to_string();
    for (name, marker) in prelude::BELAY_LOCATION_MARKERS {
        let value = match marker {
            m if m == prelude::BELAY_PROJECT_ROOT_TOKEN => project_root.clone(),
            m if m == prelude::BELAY_CODE_ROOT_TOKEN => code_root.clone(),
            m if m == prelude::BELAY_SHAPE_ROOT_TOKEN => shape_root.clone(),
            _ => name.to_string(),
        };
        out = out.replace(marker, &value);
    }
    out
}
