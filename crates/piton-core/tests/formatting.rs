//! Formatting must not change what a file means.
//!
//! Piton is whitespace-structured, so a formatter that re-indents is rewriting
//! the very thing that carries meaning. Idempotence is not enough on its own:
//! a formatter can be perfectly stable and still be wrong.
//!
//! The one change allowed is to the spaces between words in prose, which the
//! formatter collapses, so compiled strings are compared with runs of spaces
//! collapsed too.

use piton_core::compile::compile;
use piton_core::db::Db;
use piton_core::framework::Frameworks;
use piton_core::serialize::to_json;
use piton_core::{builtin, FileId};

/// Compile a source string into the JSON of all its top-level names.
fn compiled(source: &str) -> Option<serde_json::Value> {
    let mut db = Db::new();
    for module in builtin::modules() {
        db.add_virtual_module(module.name, module.source);
    }
    db.add_virtual_module("@test/main", source);
    let entry = db.resolve(FileId(0), "@test/main").ok()?;
    let compilation = compile(db, vec![entry], &Frameworks::default());
    if compilation.diagnostics.has_errors() {
        return None;
    }
    let mut document = serde_json::Map::new();
    for (name, value) in compilation.file_values(entry) {
        document.insert(name, spacing_collapsed(to_json(&value)));
    }
    Some(serde_json::Value::Object(document))
}

/// A compiled value with every run of spaces in its strings reduced to one.
fn spacing_collapsed(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(text) => {
            let mut out = String::new();
            for ch in text.chars() {
                if !(ch == ' ' && out.ends_with(' ')) {
                    out.push(ch);
                }
            }
            Value::String(out)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(spacing_collapsed).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, spacing_collapsed(v))).collect()),
        other => other,
    }
}

/// Sources whose meaning must survive the formatter, covering every shape
/// where indentation, blank lines, or line breaks carry meaning.
const CORPUS: &[&str] = &[
    "a: 1\n",
    "a: 1 // a comment\n",
    "// leading comment\na: 1\n",
    "a:\n    one\n    two\n",
    "a:\n    one\n\n    two\n",
    "a:\n    one\n\n\n    two\n",
    "a:\n    - one\n    - two\n",
    "a:\n    - one\n        - nested\n            - deeper\n",
    "a:\n    - key: value\n    - other: thing\n    - plain\n",
    "a:\n  - outer:\n      nested: value\n",
    "a:\n    - key:: string: value\n",
    "a:\n  key:\n      prose inset past one unit\n",
    "a: [one, two, [three]]\n",
    "combined:\n    prose\n\n    - one\n    - two\n\n    key:\n        deep: 1\n",
    "a:\n    b:\n        c:\n            d: deep\n",
    "a:: string:: number: 42\n",
    "abstract anchor S as s:\n    x:: string\n\ns C:\n    x: value\n",
    "anchor A:\n    name: A\n\nanchor B extends A:\n    other: ${super.name}\n",
    "anchor A:\n    items:\n        - one\n\nanchor B extends A:\n    items:\n        + {super.items}\n        - two\n",
    "anchor A:\n    s:\n        a: 1\n\nanchor B extends A:\n    s:\n        + {super.s}\n        b: 2\n",
    "a: {1 + 2 * 3}\n",
    "a: Hello, ${name}!\nname: world\n",
    "a: he said \"hi\" and left\n",
    "a: see https://example.com/a//b now\n",
    "a: a well-known thing\n",
    "a:\n\tone\n\ttwo\n",
    "a: 1\r\nb: 2\r\n",
    "note:\n    A paragraph that runs\n    across two lines.\n\n    And a second one.\n",
    // Prose rewrapping: the compiled text must not change.
    "note:\n    This line of prose runs on well past the eightieth column of the file, so it wraps around.\n",
    "note:\n    Short\n    lines\n    join.\n",
    "note:\n    First sentence ends.  Second sentence keeps two spaces before it and runs on past the edge.\n",
    "note:\n    Values like {1 + 2} and \"a quoted phrase with spaces\" stay whole when the line is long enough to wrap.\n",
    "note:\n    x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x - beta gamma\n",
    "note:\n    x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x x note: beta\n",
    "note:\n    Before\n    {1 + 2}\n    after the expression line comes more prose that is long enough to wrap around.\n",
    "note:\n    This sentence mentions\n    description: which stays prose because it continues a run of text past the edge.\n",
    "note:\n    A long first part of a sentence that is certainly long enough to want wrapping here\n    // an aside\n    and the rest.\n",
    "a: Hello\nnote:\n    Say ${a} to everyone who reads this line, which runs on long enough to need a wrap.\n",
    "note:\n    see https://example.com/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa now\n",
    "a:\n    - item\n    Prose after a list item that runs long enough to wrap past the eightieth column.\n",
    "rectangleExample:\n    Rectangle is a primitive type of its own, not a Polygon made by a\n    rectangle verb.  Its corner and anchor definitions are parameter            modes on the verb that builds it.\n",
    "note:\n    Run `a    b`   now, and    {1 + 2}    then   \"x   y\"   done.\n",
];

#[test]
fn formatting_preserves_meaning() {
    for source in CORPUS {
        let before = compiled(source);
        let formatted = piton_fmt::format(source);
        let after = compiled(&formatted);
        assert_eq!(
            before, after,
            "formatting changed what this means:\n--- before ---\n{source}--- after ---\n{formatted}"
        );
    }
}

#[test]
fn formatting_is_idempotent_and_leaves_valid_source() {
    for source in CORPUS {
        let once = piton_fmt::format(source);
        let twice = piton_fmt::format(&once);
        assert_eq!(once, twice, "not idempotent:\n{source}");
        assert!(
            compiled(&once).is_some(),
            "formatting produced source that no longer compiles:\n{once}"
        );
    }
}

#[test]
fn formatting_keeps_the_line_endings_it_was_given() {
    let crlf = "anchor A:\r\n  x: 1\r\n  y: 2\r\n";
    let formatted = piton_fmt::format(crlf);
    assert!(formatted.contains("\r\n"), "a CRLF file must stay CRLF:\n{formatted:?}");
    assert!(!formatted.contains("\n\r"), "{formatted:?}");
    assert_eq!(formatted.matches("\r\n").count(), formatted.matches('\n').count());

    let lf = "anchor A:\n  x: 1\n";
    assert!(!piton_fmt::format(lf).contains('\r'), "an LF file must stay LF");
}
