//! TypeScript to JavaScript, in-process.
//!
//! Plugins are written in TypeScript, and nobody running `piton` should need
//! Node or `tsc` installed to use one. oxc strips the types: the parser reads
//! the TypeScript, semantic analysis gives the transformer the scopes it
//! needs, the transformer removes everything that is only types, and the code
//! generator prints the JavaScript QuickJS runs. Nothing is down-levelled:
//! QuickJS runs modern JavaScript as it is.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_codegen::Codegen;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer::{TransformOptions, Transformer};

/// The JavaScript for one source file. A `.js` or `.mjs` file is returned as
/// it is; anything else is read as TypeScript.
pub fn transpile(path: &str, source: &str) -> Result<String, String> {
    if path.ends_with(".js") || path.ends_with(".mjs") {
        return Ok(source.to_string());
    }
    let allocator = Allocator::default();
    let source_type = SourceType::ts();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(first_error(path, source, &parsed.diagnostics));
    }
    let mut program = parsed.program;
    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
    let options = TransformOptions::default();
    let transformed = Transformer::new(&allocator, Path::new(path), &options)
        .build_with_scoping(scoping, &mut program);
    if !transformed.diagnostics.is_empty() {
        return Err(first_error(path, source, &transformed.diagnostics));
    }
    Ok(Codegen::new().build(&program).code)
}

/// The first problem oxc found, with the line it is on.
fn first_error(path: &str, source: &str, diagnostics: &[oxc_diagnostics::OxcDiagnostic]) -> String {
    let diagnostic = &diagnostics[0];
    let line = diagnostic
        .labels
        .as_ref()
        .first()
        .map(|label| source[..(label.offset() as usize).min(source.len())].lines().count().max(1));
    match line {
        Some(line) => format!("{path}:{line}: {diagnostic}"),
        None => format!("{path}: {diagnostic}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_are_stripped() {
        let js = transpile(
            "a.ts",
            "interface A { x: number }\nexport function f(a: A, b?: string): number { return a.x as number; }\ntype T = string;\nexport const y: T = 'y';\n",
        )
        .expect("transpiles");
        assert!(!js.contains("interface"), "{js}");
        assert!(!js.contains(": number"), "{js}");
        assert!(js.contains("export function f(a, b)"), "{js}");
    }

    #[test]
    fn syntax_errors_name_the_line() {
        let error = transpile("bad.ts", "const a = 1;\nconst = ;\n").unwrap_err();
        assert!(error.starts_with("bad.ts:2:"), "{error}");
    }
}
