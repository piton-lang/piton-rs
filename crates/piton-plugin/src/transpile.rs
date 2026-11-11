//! This module changes TypeScript to JavaScript in the same process.
//!
//! Plugins use TypeScript. A user of `piton` must not need Node or `tsc` to
//! use a plugin. oxc removes the types. The parser reads the TypeScript.
//! Semantic analysis gives the transformer the scopes that it needs. The
//! transformer removes all code that is only types. The code generator prints
//! the JavaScript that QuickJS runs. This module does not change the
//! JavaScript to an older version. QuickJS runs modern JavaScript as it is.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_codegen::Codegen;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer::{TransformOptions, Transformer};

/// Gives the JavaScript for one source file. A `.js` or `.mjs` file comes
/// back as it is. This function reads all other files as TypeScript.
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

/// Gives the first problem that oxc found, and the line of the problem.
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
