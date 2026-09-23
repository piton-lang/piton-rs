//! The one place the CLI renders a compiled module.
//!
//! `piton compile` and `piton build` both turn a module's compiled surface into
//! renderer output. Keeping the call to `piton_emit::render` here means a
//! change to how rendering is set up -- what context a renderer is given --
//! is made once.

use std::path::Path;

use piton_compile::{prelude, Compilation, Framework, ModuleId};
use piton_emit::{Adapter, MarkdownContext};

/// Parses a renderer name as the command line or configuration writes it.
pub fn parse_renderer(name: &str) -> Result<Adapter, String> {
    name.parse::<Adapter>().map_err(|_| {
        format!("unknown renderer `{name}`; valid options are json, yaml, markdown")
    })
}

/// Renders what a module compiles to.
///
/// `output_directory` is the directory the rendered file will be read from:
/// where `--write` or a build puts it, or the source file's own directory when
/// it goes to stdout. Links a renderer writes are relative to it.
/// `output_root` is where the output tree starts when it isn't next to the
/// sources, as with a build, so a reference points at the other file's output.
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

/// Fills in Belay's special imports for plain renderer output.
///
/// They are paths relative to the file they end up in. The project, code, and
/// shape roots are known without an adapter; the agent's directory and its
/// compiled shape only exist for an adapter, so outside one they are written
/// as their names rather than as the compiler's internal markers.
fn resolve_locations(compilation: &Compilation, text: &str, output_directory: &Path) -> String {
    if !text.contains('\u{e000}') {
        return text.to_string();
    }
    let project = &compilation.project;
    let belay = project.frameworks.iter().find_map(|framework| match framework {
        Framework::Belay(config) => Some(config),
        #[allow(unreachable_patterns)]
        _ => None,
    });
    let relative = |target: &Path| {
        piton_emit::relative_file(output_directory, &target.join("x"))
            .trim_end_matches("/x")
            .to_string()
    };
    let project_root = relative(&project.root);
    let code_root = belay.map_or_else(|| project_root.clone(), |c| relative(&c.code_root));
    let shape_root = belay
        .and_then(|c| c.shape_root.as_deref())
        .map_or_else(|| project_root.clone(), |p| relative(p));
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
