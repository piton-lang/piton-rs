//! Generated Piton documentation.
//!
//! The reference below is assembled from the compiler's own keyword, type, and
//! operator tables and from each registered framework's module source, so it
//! cannot drift away from what the compiler actually accepts.

pub mod model;

use piton_core::framework::Frameworks;
use piton_syntax::kind::{BUILTIN_TYPES, LITERAL_KEYWORDS, RESERVED_KEYWORDS};

pub use model::{describe, AnchorDoc, FrameworkDoc};

/// A compact brief suitable for appending to an agent's system prompt.
pub fn fluency_brief(frameworks: &Frameworks) -> String {
    let docs = describe(frameworks);
    let mut out = String::new();
    out.push_str(HEADER);
    out.push_str(&core_reference());
    out.push_str(&framework_reference(&docs));
    out.push_str(WORKFLOW);
    out
}

/// A Claude Code skill wrapping the same brief.
///
/// The description names whichever frameworks are registered, so the skill
/// describes the compiler the user actually has.
pub fn skill_document(frameworks: &Frameworks) -> String {
    let docs = describe(frameworks);
    let mut keywords: Vec<String> = docs.iter().flat_map(|doc| doc.keywords()).collect();
    keywords.sort();
    keywords.dedup();
    let extra = if keywords.is_empty() {
        String::new()
    } else {
        format!(" Also use for the `{}` keywords.", keywords.join("`, `"))
    };
    format!(
        "---\nname: piton\ndescription: Write, read, and compile Piton (.pi) source. Use when \
         working with .pi files, piton.config.pi, anchors, or Piton frameworks.{extra}\n---\n\n{}",
        fluency_brief(frameworks)
    )
}

/// The long-form language reference, for `docs/language.md`.
pub fn language_reference(frameworks: &Frameworks) -> String {
    let docs = describe(frameworks);
    format!(
        "# The Piton Language\n\n{}{}{}",
        LONG_INTRO,
        core_reference(),
        framework_reference(&docs)
    )
}

const HEADER: &str = "\
# Piton fluency

Piton is a declarative, whitespace-structured language for describing agentic
skills, agents, commands, and reference documents. It has no runtime: it
compiles to data (JSON, YAML) or, through a framework, to Markdown. Source
files use the `.pi` extension.

";

const LONG_INTRO: &str = "\
Piton is a declarative, whitespace-structured language for describing systems in
a mix of prose and structure. It has no runtime; a Piton program compiles to
data or, through a framework, to documents.

";

const WORKFLOW: &str = "\
## Working with a Piton project

- `piton check <glob>` reports errors without writing anything.
- `piton compile <glob>` writes JSON or YAML next to each input.
- `piton build` builds the project described by `piton.config.pi`.
- `piton build check` builds and reports without writing.
- `piton format <glob>` applies the canonical style: four spaces, one space
  after `//`, sorted and wrapped import lists.
- `piton lsp` runs the language server.

## Writing Piton well

- Prefer an abstract anchor exported `as` a keyword for anything that will be
  implemented repeatedly; it documents intent and prevents a concrete anchor
  from implementing two abstracts.
- Reach for `+ {super.x}` rather than repeating a base's list.
- Use `this` when a base wants its own value and `self` when it wants the
  most-derived one.
- Keep prose as prose. Only reach for `{ }` when a value genuinely needs to be
  computed or referenced.
";

/// The parts of the reference that come from the compiler's kind tables.
fn core_reference() -> String {
    let mut out = String::new();
    out.push_str("## Syntax\n\n");
    out.push_str(
        "```piton\n\
         // A comment. Comments are line-only.\n\
         myVariable: 42                       // a number\n\
         myString: Strings are unquoted       // a string\n\
         myQuoted: \"false\"                    // quoting forces a string\n\
         myTyped:: string:: number: 42        // constraints, first match wins\n\
         \n\
         myList:\n\
         \x20   - one\n\
         \x20   - two\n\
         myInlineList: [one, two, three]\n\
         \n\
         myDictionary:\n\
         \x20   nested:\n\
         \x20       deep: value\n\
         \n\
         computed: {1 + 2}                    // `{ }` evaluates; bare text does not\n\
         interpolated: Hello, ${myString}\n\
         \n\
         anchor Base:\n\
         \x20   name: Base\n\
         \x20   summary: This is ${this.name}  // `this` pins to Base\n\
         \n\
         anchor Child extends Base:\n\
         \x20   name: Child\n\
         \x20   detail: This is ${self.name}   // `self` follows the child\n\
         \x20   summary: ${super.summary} and more\n\
         \n\
         abstract anchor Shape as shape:      // `as` registers a keyword\n\
         \x20   description:: string\n\
         \n\
         shape Concrete:                      // same as `extends Shape`\n\
         \x20   description: Implemented\n\
         \n\
         from ./other import Thing, Other Alias\n\
         from ./other export *\n\
         use ./keywords\n\
         export anchor Published:\n\
         \x20   value: 1\n\
         ```\n\n",
    );

    out.push_str("## Reserved words\n\n");
    out.push_str(&format!("`{}`\n\n", RESERVED_KEYWORDS.join("`, `")));
    out.push_str("## Types\n\n");
    out.push_str(&format!("`{}`\n\n", BUILTIN_TYPES.join("`, `")));
    out.push_str(&format!("Literals: `{}`\n\n", LITERAL_KEYWORDS.join("`, `")));
    out.push_str(RULES);
    out
}

const RULES: &str = "\
## Rules worth memorising

- A value is an expression only when the *whole* value parses as one over
  literal atoms. `1 + 2` is `3`; `a + b` is the string `a + b`; use `{a + b}`
  to reference names.
- `+` on lists concatenates and deduplicates, right operand winning:
  `[1,2,3,4] + [1,2,3]` is `[4,1,2,3]`. `++` keeps duplicates.
- `+` merges dictionaries shallowly; `++` merges them deeply.
- Mixing unrelated types with `+` produces a mixed list, not an error.
- A block that mixes prose, `- ` items, and `key:` pairs becomes an implicit
  list; the keys written directly in it stay addressable.
- Inheritance is left to right with the right-most base winning; a child always
  wins over its bases; a declaring keyword is the left-most base.
- A concrete anchor may implement at most one abstract anchor, and must supply
  every abstract property left without a value.
- Strings never coerce to `boolean` or `number`; simple values do coerce to
  `string`.
- Modulo uses floor semantics; division by zero and comparing mismatched types
  are compile errors.
- Two files may import from each other. A circular reference is an error only
  when it cannot settle: `${A}` and `${B}` naming each other is fine, because an
  interpolated anchor renders as its name, while `A: {B}` and `B: {A}` is not.
- Prose lines join with a space and a blank line starts a new line. A fenced
  code block, opened on an indented line by three or more backticks or tildes
  and closed by a line of at least as many of the same, is taken exactly as
  written instead: no comments, escapes, keys, or interpolations inside, its
  line breaks and relative indentation kept, its fences part of the string.
- Indentation must be consistent within a file; the canonical style is four
  spaces.

";

/// The parts of the reference that come from each framework's own module.
fn framework_reference(docs: &[FrameworkDoc]) -> String {
    if docs.is_empty() {
        return String::new();
    }
    let mut out = String::from("## Frameworks\n\n");
    for doc in docs {
        out.push_str(&format!(
            "### `{}` (framework `{}`)\n\n`use {}` for its keywords, `from {} import ...` for \
             its anchors.\n\n",
            doc.module, doc.name, doc.module, doc.module
        ));
        if !doc.sigils.is_empty() {
            out.push_str(&format!(
                "Interpolation sigils: {}\n\n",
                doc.sigils
                    .iter()
                    .map(|sigil| format!("`{sigil}{{}}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        for anchor in &doc.anchors {
            let keyword = match &anchor.keyword {
                Some(keyword) => format!(" — keyword `{keyword}`"),
                None => String::new(),
            };
            let kind = if anchor.is_abstract { "abstract anchor" } else { "anchor" };
            out.push_str(&format!("- **{}** ({kind}){keyword}", anchor.name));
            if let Some(summary) = &anchor.doc {
                out.push_str(&format!(": {}", summary.replace('\n', " ")));
            }
            out.push('\n');
            for (name, constraint) in &anchor.properties {
                match constraint {
                    Some(constraint) => out.push_str(&format!("  - `{name}:: {constraint}`\n")),
                    None => out.push_str(&format!("  - `{name}`\n")),
                }
            }
        }
        for var in &doc.vars {
            out.push_str(&format!("- **{}** (variable)\n", var.name));
        }
        out.push('\n');
    }
    out
}
