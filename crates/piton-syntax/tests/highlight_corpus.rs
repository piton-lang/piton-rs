//! Every syntax highlighter, run over one corpus and compared with the parser.
//!
//! `editors/` ships a tree-sitter grammar, a TextMate grammar, a Sublime
//! syntax, a Kate definition and a Vim syntax file. Each is an approximation of
//! the parser written in a different regex dialect, and checking them by
//! reading their source only catches a missing keyword. Here each one is run,
//! by the tool that runs it in an editor where that is possible, over the
//! fixtures in `editors/fixtures/`, and what it paints every byte is compared
//! with what the parser says the byte is.
//!
//! The parser's answer is `piton_syntax::highlight::classify`, written out as
//! `editors/fixtures/<name>.expected`. A tool's answer is a scope, capture,
//! item or group name per byte, read back to a class through
//! `editors/fixtures/scopes.toml`. Where a format cannot do what the parser
//! does -- a regex engine that cannot see the line above, say -- the deviation
//! is written down in `editors/fixtures/gaps.toml` with its reason, and an entry
//! there has to keep failing, so the list cannot go stale.
//!
//! The tools are not all installed everywhere. A runner whose tool is missing
//! says so and passes, unless `PITON_REQUIRE_EDITOR_TOOLS=1`, which CI sets.
//!
//! `PITON_BLESS=1 cargo test -p piton-syntax --test highlight_corpus`
//! rewrites the `.expected` files from the parser, and the review happens on
//! the diff.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use piton_syntax::highlight::{classify, render_expected, Class};
use piton_syntax::parser::parse;

// ---------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn editors() -> PathBuf {
    repo_root().join("editors")
}

fn fixtures_dir() -> PathBuf {
    editors().join("fixtures")
}

/// One fixture: its source, and the class the parser gives every byte of it.
struct Fixture {
    name: String,
    path: PathBuf,
    source: String,
    golden: Vec<Option<Class>>,
}

/// What every byte is, with `None` for plain text.
type Painting = Vec<Option<Class>>;

fn fixtures() -> Vec<Fixture> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(fixtures_dir())
        .expect("editors/fixtures exists")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "pi"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "editors/fixtures holds no .pi files");

    paths
        .into_iter()
        .map(|path| {
            let source = std::fs::read_to_string(&path).expect("fixture is UTF-8");
            let parsed = parse(&source, &path);
            let mut golden = vec![None; source.len()];
            for (span, class) in classify(&parsed) {
                for slot in &mut golden[span.start..span.end] {
                    *slot = Some(class);
                }
            }
            Fixture {
                name: path.file_stem().unwrap().to_string_lossy().to_string(),
                path,
                source,
                golden,
            }
        })
        .collect()
}

/// The canonical name of every class in one format, read from `scopes.toml`.
///
/// The table is keyed by class; a runner needs it the other way round, from the
/// name a tool printed to the class it stands for.
fn reverse_map(format: &str) -> HashMap<String, Class> {
    let mut out = HashMap::new();
    for (class, name) in scope_table(format) {
        let previous = out.insert(name.clone(), class);
        assert!(
            previous.is_none(),
            "scopes.toml gives `{name}` to two classes in the `{format}` column"
        );
    }
    out
}

/// `(class, name)` for every row of `scopes.toml`, in one format's column.
fn scope_table(format: &str) -> Vec<(Class, String)> {
    let text = std::fs::read_to_string(fixtures_dir().join("scopes.toml")).expect("scopes.toml");
    let table: toml::Table = text.parse().expect("scopes.toml is TOML");
    let mut out = Vec::new();
    for (class_name, row) in &table {
        let class = Class::from_name(class_name)
            .unwrap_or_else(|| panic!("scopes.toml has a row for `{class_name}`, which is no class"));
        let name = row
            .get(format)
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| panic!("scopes.toml: `{class_name}` has no `{format}` name"));
        out.push((class, name.to_string()));
    }
    for class in Class::ALL {
        assert!(
            out.iter().any(|(listed, _)| listed == class),
            "scopes.toml has no row for `{}`",
            class.name()
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Comparison and gaps
// ---------------------------------------------------------------------------

/// A line of a fixture on which one format is allowed to disagree.
#[derive(Debug)]
struct Gap {
    format: String,
    fixture: String,
    line: usize,
}

fn gaps(format: &str) -> Vec<Gap> {
    let text = std::fs::read_to_string(fixtures_dir().join("gaps.toml")).expect("gaps.toml");
    let table: toml::Table = text.parse().expect("gaps.toml is TOML");
    let Some(entries) = table.get("gap") else {
        return Vec::new();
    };
    let entries = entries.as_array().expect("`gap` is an array of tables");
    let mut out = Vec::new();
    for entry in entries {
        let field = |key: &str| {
            entry
                .get(key)
                .unwrap_or_else(|| panic!("a gap has no `{key}`: {entry}"))
                .clone()
        };
        let reason = field("reason");
        assert!(
            reason.as_str().is_some_and(|reason| !reason.trim().is_empty()),
            "every gap says why: {entry}"
        );
        let gap = Gap {
            format: field("format").as_str().expect("format is a string").to_string(),
            fixture: field("fixture").as_str().expect("fixture is a string").to_string(),
            line: field("line").as_integer().expect("line is a number") as usize,
        };
        if gap.format == format {
            out.push(gap);
        }
    }
    out
}

/// One stretch of a line where a tool and the parser disagree.
struct Mismatch {
    line: usize,
    start: usize,
    end: usize,
    expected: Option<Class>,
    actual: Option<Class>,
}

fn class_name(class: Option<Class>) -> &'static str {
    class.map_or("Plain", Class::name)
}

/// Compares a tool's painting of every fixture with the parser's, and fails
/// with the first disagreements unless `gaps.toml` excuses their lines.
///
/// Whitespace is not compared: a tool may paint the space between a key and its
/// value either way, and nobody can see the difference.
fn compare(format: &str, paintings: &[(&Fixture, Painting)]) {
    let gaps = gaps(format);
    let mut problems: Vec<String> = Vec::new();
    let mut excused: Vec<(String, usize)> = Vec::new();

    for gap in &gaps {
        assert!(
            paintings.iter().any(|(fixture, _)| fixture.name == gap.fixture),
            "gaps.toml names the fixture `{}`, which does not exist",
            gap.fixture
        );
    }

    for (fixture, actual) in paintings {
        assert_eq!(actual.len(), fixture.source.len(), "{format} painted the wrong length");
        let mut mismatches: Vec<Mismatch> = Vec::new();
        let mut line = 1;
        let mut line_start = 0;
        for (offset, byte) in fixture.source.bytes().enumerate() {
            if byte == b'\n' {
                line += 1;
                line_start = offset + 1;
                continue;
            }
            if byte.is_ascii_whitespace() {
                continue;
            }
            let (expected, got) = (fixture.golden[offset], actual[offset]);
            if expected == got {
                continue;
            }
            let column = offset - line_start;
            match mismatches.last_mut() {
                Some(last)
                    if last.line == line
                        && last.expected == expected
                        && last.actual == got
                        && fixture.source[line_start + last.end..offset]
                            .bytes()
                            .all(|b| b.is_ascii_whitespace()) =>
                {
                    last.end = column + 1;
                }
                _ => mismatches.push(Mismatch {
                    line,
                    start: column,
                    end: column + 1,
                    expected,
                    actual: got,
                }),
            }
        }

        let lines: Vec<&str> = fixture.source.lines().collect();
        for gap in gaps.iter().filter(|gap| gap.fixture == fixture.name) {
            if mismatches.iter().any(|m| m.line == gap.line) {
                excused.push((fixture.name.clone(), gap.line));
            } else {
                problems.push(format!(
                    "{}.pi:{}: gaps.toml excuses this line for {format}, but it now \
                     matches the parser. Remove the entry.\n    {}",
                    fixture.name,
                    gap.line,
                    lines.get(gap.line - 1).unwrap_or(&"")
                ));
            }
        }
        for mismatch in mismatches {
            if gaps
                .iter()
                .any(|gap| gap.fixture == fixture.name && gap.line == mismatch.line)
            {
                continue;
            }
            let text = lines.get(mismatch.line - 1).copied().unwrap_or("");
            problems.push(format!(
                "{}.pi:{}:{}-{}: `{}` is {} but {format} paints it {}\n    {}",
                fixture.name,
                mismatch.line,
                mismatch.start,
                mismatch.end,
                text.get(mismatch.start..mismatch.end).unwrap_or("?"),
                class_name(mismatch.expected),
                class_name(mismatch.actual),
                text
            ));
        }
    }

    if !problems.is_empty() {
        let total = problems.len();
        problems.truncate(20);
        panic!(
            "{format} disagrees with the parser in {total} place(s) (first {} shown).\n\
             Fix the definition, or, if the format cannot express the rule, add a \
             `[[gap]]` to editors/fixtures/gaps.toml saying why.\n\n{}",
            problems.len(),
            problems.join("\n")
        );
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Whether a tool can be run. A missing one skips its runner with a message,
/// unless CI has asked for every tool to be present.
fn have(tool: &str, version_flag: &str) -> bool {
    let found = Command::new(tool)
        .arg(version_flag)
        .output()
        .is_ok_and(|output| output.status.success());
    if !found {
        if std::env::var("PITON_REQUIRE_EDITOR_TOOLS").is_ok_and(|value| value == "1") {
            panic!("{tool} not found, and PITON_REQUIRE_EDITOR_TOOLS=1 requires it");
        }
        eprintln!("skipped: {tool} not found");
    }
    found
}

fn run(command: &mut Command) -> String {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("cannot run {command:?}: {error}"));
    assert!(
        output.status.success(),
        "{command:?} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("tool output is UTF-8")
}

/// A fresh scratch directory for one runner.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("highlight-corpus").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create directory");
    for entry in std::fs::read_dir(from).expect("read directory").flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// Byte offset of the start of every line.
fn line_starts(source: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(source.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

// ---------------------------------------------------------------------------
// Runners
// ---------------------------------------------------------------------------

/// The parser's own classification, against the files a reviewer reads.
///
/// Also runs the classifier over the whole specification, which is the largest
/// body of real Piton there is, to make sure it neither panics nor leaves a
/// class run straddling a line.
#[test]
fn golden() {
    let bless = std::env::var("PITON_BLESS").is_ok_and(|value| value == "1");
    let mut stale = Vec::new();
    for fixture in fixtures() {
        let parsed = parse(&fixture.source, &fixture.path);
        assert!(
            !parsed.has_errors(),
            "{} is not valid Piton, and the corpus only tests valid files: {:?}",
            fixture.path.display(),
            parsed.diagnostics
        );
        let rendered = render_expected(&fixture.source, &classify(&parsed));
        let expected_path = fixture.path.with_extension("expected");
        let current = std::fs::read_to_string(&expected_path).unwrap_or_default();
        if bless {
            if current != rendered {
                std::fs::write(&expected_path, &rendered).expect("write .expected");
            }
        } else if current != rendered {
            stale.push(fixture.name.clone());
        }
    }
    assert!(
        stale.is_empty(),
        "the classifier no longer writes these .expected files: {}. Run \
         `PITON_BLESS=1 cargo test -p piton-syntax --test highlight_corpus` and \
         review the diff.",
        stale.join(", ")
    );

    for path in spec_files(&repo_root().join("spec")) {
        let source = std::fs::read_to_string(&path).expect("spec file");
        let parsed = parse(&source, &path);
        let classes = classify(&parsed);
        let mut previous_end = 0;
        for (span, _) in &classes {
            assert!(
                span.start >= previous_end && span.end <= source.len(),
                "{}: classify returned overlapping or out-of-range spans",
                path.display()
            );
            previous_end = span.end;
        }
        // `render_expected` cuts runs at line breaks; it must never need to.
        for (span, class) in &classes {
            assert!(
                !source[span.start..span.end].contains('\n'),
                "{}: a {} token spans a line break at {}..{}",
                path.display(),
                class.name(),
                span.start,
                span.end
            );
        }
    }
}

fn spec_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(spec_files(&path));
        } else if path.extension().is_some_and(|e| e == "pi") {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// The tree-sitter grammar and its `highlights.scm`, which Zed, Helix, Neovim
/// and Emacs all read.
///
/// The grammar is generated and compiled in a scratch copy, because
/// `editors/tree-sitter-piton/src` is generated output the repository does not
/// keep, and the query is run by the CLI. A capture paints its node; a node
/// inside another captured node paints over it, which is what every editor
/// does with nested captures. Two captures on one node are a failure on their
/// own, because which one wins differs between Neovim and tree-sitter-highlight.
#[test]
fn tree_sitter() {
    if !have("tree-sitter", "--version") {
        return;
    }
    let dir = scratch("tree-sitter");
    let grammar = editors().join("tree-sitter-piton");
    for file in ["grammar.js", "package.json"] {
        std::fs::copy(grammar.join(file), dir.join(file)).expect("copy grammar");
    }
    copy_dir(&grammar.join("queries"), &dir.join("queries"));
    run(Command::new("tree-sitter").arg("generate").current_dir(&dir));

    let names = reverse_map("ts");
    let all = fixtures();
    let mut paintings = Vec::new();
    for fixture in &all {
        let output = run(Command::new("tree-sitter")
            .args(["query", "--captures", "-p"])
            .arg(&dir)
            .arg(dir.join("queries/highlights.scm"))
            .arg(&fixture.path)
            .current_dir(&dir));
        let starts = line_starts(&fixture.source);
        let offset = |row: usize, column: usize| starts[row] + column;

        let mut captures: Vec<(usize, usize, String)> = Vec::new();
        for line in output.lines() {
            // With `--captures` each line starts `pattern: N, capture: ...`.
            let Some((_, rest)) = line.split_once("capture: ") else {
                continue;
            };
            // `capture: 3 - keyword, start: (0, 0), end: (0, 3), text: ...`,
            // where the index is left out for some captures.
            let rest = match rest.split_once(" - ") {
                Some((index, rest)) if index.bytes().all(|b| b.is_ascii_digit()) => rest,
                _ => rest,
            };
            let (name, rest) = rest.split_once(", start: (").expect("capture start");
            let (start, rest) = rest.split_once("), end: (").expect("capture end");
            let end = rest.split_once(')').expect("capture end").0;
            let point = |text: &str| {
                let (row, column) = text.split_once(", ").expect("row, column");
                offset(row.parse().unwrap(), column.parse().unwrap())
            };
            captures.push((point(start), point(end), name.to_string()));
        }

        let mut seen: BTreeMap<(usize, usize), String> = BTreeMap::new();
        for (start, end, name) in &captures {
            if let Some(other) = seen.insert((*start, *end), name.clone()) {
                if &other != name {
                    panic!(
                        "{}.pi: `{}` is captured as both @{other} and @{name}. One \
                         node, one capture: editors disagree on which wins.",
                        fixture.name,
                        &fixture.source[*start..*end]
                    );
                }
            }
        }

        // Outer nodes first, so that an inner capture paints over them.
        captures.sort_by_key(|(start, end, _)| std::cmp::Reverse(end - start));
        let mut painting: Painting = vec![None; fixture.source.len()];
        for (start, end, name) in captures {
            let class = names.get(&name).copied();
            for slot in &mut painting[start..end.min(fixture.source.len())] {
                *slot = class;
            }
        }
        paintings.push((fixture, painting));
    }
    compare("tree-sitter", &paintings);
}

/// The TextMate grammar, through VS Code's own engine.
#[test]
fn textmate() {
    let vscode = editors().join("vscode");
    if !have("node", "--version") {
        return;
    }
    if !vscode.join("node_modules/vscode-textmate").is_dir() {
        if std::env::var("PITON_REQUIRE_EDITOR_TOOLS").is_ok_and(|value| value == "1") {
            panic!("editors/vscode/node_modules is not installed; run `npm ci` there");
        }
        eprintln!("skipped: editors/vscode/node_modules not installed (run `npm ci` there)");
        return;
    }
    let names = reverse_map("tm");
    let all = fixtures();
    let mut paintings = Vec::new();
    for fixture in &all {
        let output = run(Command::new("node")
            .arg(vscode.join("scripts/tokenize.mjs"))
            .arg(vscode.join("syntaxes/piton.tmLanguage.json"))
            .arg(&fixture.path));
        let tokens: Vec<(usize, usize, usize, Vec<String>)> =
            serde_json::from_str(&output).expect("tokenize.mjs prints JSON");
        let starts = line_starts(&fixture.source);
        let mut painting: Painting = vec![None; fixture.source.len()];
        for (line, start, end, scopes) in tokens {
            let text = &fixture.source[starts[line]..];
            let class = innermost(&scopes, &names);
            let (start, end) = (utf16_to_byte(text, start), utf16_to_byte(text, end));
            for slot in &mut painting[starts[line] + start..starts[line] + end] {
                *slot = class;
            }
        }
        paintings.push((fixture, painting));
    }
    compare("textmate", &paintings);
}

/// The innermost scope in a stack that is exactly a canonical name.
///
/// Exactly, not by prefix: `string.unquoted.verbatim.piton` must not be read as
/// a quoted string, and a scope the table does not name is plain text however
/// it is spelled.
fn innermost<S: AsRef<str>>(scopes: &[S], names: &HashMap<String, Class>) -> Option<Class> {
    scopes
        .iter()
        .rev()
        .find_map(|scope| names.get(scope.as_ref()).copied())
}

fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (offset, ch) in text.char_indices() {
        if seen >= units {
            return offset;
        }
        seen += ch.len_utf16();
    }
    text.len()
}

/// The Sublime syntax, through `syntect`.
///
/// syntect is a reimplementation of Sublime's engine, not Sublime itself, so a
/// place where the two differ is recorded as a gap rather than worked around in
/// the definition. It needs no tool installed, so this runner never skips.
#[test]
fn sublime() {
    use syntect::parsing::{ParseState, ScopeStack, SyntaxDefinition, SyntaxSetBuilder};

    let path = editors().join("sublime/Piton.sublime-syntax");
    let text = std::fs::read_to_string(&path).expect("Piton.sublime-syntax");
    let mut builder = SyntaxSetBuilder::new();
    builder.add(
        SyntaxDefinition::load_from_str(&text, true, Some("Piton"))
            .unwrap_or_else(|error| panic!("{} does not load: {error}", path.display())),
    );
    // The fences embed JSON and YAML, which ship with Sublime rather than with
    // this package. They are present here, and empty.
    for (name, scope) in [("JSON", "source.json"), ("YAML", "source.yaml")] {
        let stub = format!(
            "%YAML 1.2\n---\nname: {name}\nscope: {scope}\ncontexts:\n  main: []\n"
        );
        builder.add(SyntaxDefinition::load_from_str(&stub, true, None).expect("stub syntax"));
    }
    let set = builder.build();
    let syntax = set.find_syntax_by_name("Piton").expect("the Piton syntax");

    let table = reverse_map("tm");
    let names: HashMap<syntect::parsing::Scope, Class> = table
        .iter()
        .map(|(name, class)| (syntect::parsing::Scope::new(name).expect("a valid scope"), *class))
        .collect();

    let all = fixtures();
    let mut paintings = Vec::new();
    for fixture in &all {
        let mut state = ParseState::new(syntax);
        let mut stack = ScopeStack::new();
        let mut painting: Painting = vec![None; fixture.source.len()];
        let mut line_start = 0;
        // Sublime feeds each line with its newline, which `$` and `\n` in a
        // pattern rely on.
        for line in fixture.source.split_inclusive('\n') {
            let ops = state
                .parse_line(line, &set)
                .unwrap_or_else(|error| panic!("{}: {error}", fixture.name));
            let mut position = 0;
            let paint = |stack: &ScopeStack, from: usize, to: usize, painting: &mut Painting| {
                let class = stack
                    .as_slice()
                    .iter()
                    .rev()
                    .find_map(|scope| names.get(scope).copied());
                for slot in &mut painting[line_start + from..line_start + to] {
                    *slot = class;
                }
            };
            for (at, op) in ops {
                if at > position {
                    paint(&stack, position, at, &mut painting);
                    position = at;
                }
                stack.apply(&op).expect("a valid scope operation");
            }
            if line.len() > position {
                paint(&stack, position, line.len(), &mut painting);
            }
            line_start += line.len();
        }
        paintings.push((fixture, painting));
    }
    compare("sublime", &paintings);
}

/// The Vim syntax file, in Vim.
///
/// `dump.vim` asks Vim for the effective syntax group of every byte, which is
/// the group a colour scheme styles: the innermost item, with transparent ones
/// seen through, and a region's `matchgroup` on its delimiters.
#[test]
fn vim() {
    if !have("vim", "--version") {
        return;
    }
    let dir = scratch("vim");
    let script = dir.join("dump.vim");
    std::fs::write(
        &script,
        r#"let s:out = []
for s:line in range(1, line('$'))
  let s:text = getline(s:line)
  for s:col in range(1, strlen(s:text))
    call add(s:out, s:line . ' ' . s:col . ' ' . synIDattr(synID(s:line, s:col, 1), 'name'))
  endfor
endfor
call writefile(s:out, $PITON_DUMP)
qall!
"#,
    )
    .expect("dump.vim");

    let names = reverse_map("vim");
    let all = fixtures();
    let mut paintings = Vec::new();
    for fixture in &all {
        let dump = dir.join(format!("{}.dump", fixture.name));
        run(Command::new("vim")
            .args(["-es", "-N", "-u", "NONE", "-i", "NONE", "--cmd"])
            .arg(format!("set rtp^={}", editors().join("vim").display()))
            .args(["--cmd", "filetype plugin on", "-c", "syntax on", "-c"])
            // Long fixture lines must not stop the syntax engine early.
            .arg("set synmaxcol=0 | syntax sync fromstart")
            .arg("-S")
            .arg(&script)
            .arg(&fixture.path)
            .env("PITON_DUMP", &dump));
        let text = std::fs::read_to_string(&dump).expect("vim wrote the dump");
        let starts = line_starts(&fixture.source);
        let mut painting: Painting = vec![None; fixture.source.len()];
        for entry in text.lines() {
            let mut fields = entry.splitn(3, ' ');
            let line: usize = fields.next().unwrap().parse().unwrap();
            let column: usize = fields.next().unwrap().parse().unwrap();
            let group = fields.next().unwrap_or("");
            painting[starts[line - 1] + column - 1] = names.get(group).copied();
        }
        paintings.push((fixture, painting));
    }
    compare("vim", &paintings);
}

/// The Kate definition, through KSyntaxHighlighting's command-line highlighter.
///
/// `--syntax-trace=format` writes, under every line, a `|` where each format
/// starts and the format's name beneath it. That is read back into a format
/// name per character.
#[test]
fn kate() {
    if !have("ksyntaxhighlighter6", "--version") {
        return;
    }
    let dir = scratch("kate");
    let syntax = dir.join("org.kde.syntax-highlighting/syntax");
    std::fs::create_dir_all(&syntax).expect("syntax directory");
    std::fs::copy(editors().join("kate/piton.xml"), syntax.join("piton.xml")).expect("copy");

    let names = reverse_map("kate");
    let all = fixtures();
    let mut paintings = Vec::new();
    for fixture in &all {
        let output = run(Command::new("ksyntaxhighlighter6")
            .args(["-s", "Piton", "-f", "ansi", "--syntax-trace=format"])
            .arg(&fixture.path)
            .env("XDG_DATA_HOME", &dir));
        let plain = strip_ansi(&output);
        let formats = read_kate_trace(&fixture.source, &plain, &names);
        let mut painting: Painting = vec![None; fixture.source.len()];
        let mut line_start = 0;
        for (line, line_formats) in fixture.source.split('\n').zip(formats) {
            for (index, (offset, ch)) in line.char_indices().enumerate() {
                let class = line_formats.get(index).copied().flatten();
                for slot in &mut painting[line_start + offset..line_start + offset + ch.len_utf8()] {
                    *slot = class;
                }
            }
            line_start += line.len() + 1;
        }
        paintings.push((fixture, painting));
    }
    compare("kate", &paintings);
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            // A CSI sequence ends at its first byte in `@`..=`~`.
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// The class of every character of every source line, from the trace.
///
/// Each source line is followed by a marker row -- a `|` at the column where
/// each format begins -- and then label rows. A label starts at the column of
/// the marker it names; a marker whose label did not fit on a row stays a `|`
/// there and is named on a later one. Names can contain spaces (`List Marker`),
/// so a label is matched against the known names first.
fn read_kate_trace(
    source: &str,
    trace: &str,
    names: &HashMap<String, Class>,
) -> Vec<Vec<Option<Class>>> {
    let source_lines: Vec<&str> = source.split('\n').collect();
    let rows: Vec<Vec<char>> = trace.lines().map(|row| row.chars().collect()).collect();
    let mut known: Vec<&String> = names.keys().collect();
    known.sort_by_key(|name| std::cmp::Reverse(name.len()));

    let mut out = Vec::new();
    let mut row = 0;
    for source_line in &source_lines {
        let width = source_line.chars().count();
        let mut classes = vec![None; width];
        if row >= rows.len() {
            out.push(classes);
            continue;
        }
        // The source line itself. An empty line still has a marker and a
        // label below it, for the format its line break is in.
        row += 1;
        let markers: Vec<usize> = rows
            .get(row)
            .map(|marker_row| {
                marker_row
                    .iter()
                    .enumerate()
                    .filter(|(_, ch)| **ch == '|')
                    .map(|(column, _)| column)
                    .collect()
            })
            .unwrap_or_default();
        row += 1;
        let mut labels: BTreeMap<usize, String> = BTreeMap::new();
        while labels.len() < markers.len() && row < rows.len() {
            let label_row = &rows[row];
            // A label is written over the markers it is wider than, so a
            // column inside the previous label on this row starts nothing.
            let mut covered = 0;
            for &column in &markers {
                if labels.contains_key(&column) || column < covered {
                    continue;
                }
                let Some(&first) = label_row.get(column) else {
                    continue;
                };
                if first == '|' || first == ' ' {
                    continue;
                }
                let rest: String = label_row[column..].iter().collect();
                let label = known
                    .iter()
                    .find(|name| rest.starts_with(name.as_str()))
                    .map(|name| name.to_string())
                    .unwrap_or_else(|| {
                        rest.split([' ', '|']).next().unwrap_or("").to_string()
                    });
                covered = column + label.chars().count();
                labels.insert(column, label);
            }
            row += 1;
        }
        for (index, &column) in markers.iter().enumerate() {
            let end = markers.get(index + 1).copied().unwrap_or(width).min(width);
            let class = labels.get(&column).and_then(|label| names.get(label)).copied();
            for slot in classes.iter_mut().take(end).skip(column) {
                *slot = class;
            }
        }
        out.push(classes);
    }
    out
}
