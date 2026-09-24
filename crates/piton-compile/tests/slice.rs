//! Semantic slices: what one anchor, property, or variable depends on, and
//! how that is rendered.

use std::path::PathBuf;

use piton_compile::slice::{self, Entity, Entry, Relation, Slice, Target};
use piton_compile::{Compilation, Project};
use piton_core::Diagnostic;

struct Sandbox {
    dir: PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let dir = std::env::temp_dir().join(format!(
            "piton-slice-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        Sandbox { dir }
    }

    fn file(&self, relative: &str, contents: &str) -> &Sandbox {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, contents).expect("write");
        self
    }

    fn compile(&self, entry: &str) -> Compilation {
        let path = self.dir.join(entry);
        let mut project = Project::for_file(&path);
        project.source_root = self.dir.clone();
        project.root = self.dir.clone();
        let compilation = Compilation::build(project);
        let errors: Vec<String> = compilation
            .diagnostics
            .iter()
            .filter(|d| d.is_error())
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect();
        assert!(errors.is_empty(), "{errors:#?}");
        compilation
    }

    /// Resolves `file#Name[.property]` the way `piton slice` does.
    fn resolve(&self, compilation: &Compilation, target: &str) -> Result<Entity, Diagnostic> {
        let target = Target::parse(target).expect("target");
        let file = self.dir.join(target.file.as_ref().expect("file"));
        let module = compilation.graph().id_for(&file).expect("module");
        slice::resolve_in(compilation, module, &target)
    }

    fn slice(&self, target: &str) -> (Compilation, Slice) {
        let file = target.split('#').next().expect("file");
        let compilation = self.compile(file);
        let entity = self
            .resolve(&compilation, target)
            .unwrap_or_else(|d| panic!("{}: {}", d.code, d.message));
        let slice = slice::slice(&compilation, entity);
        (compilation, slice)
    }

    fn render(&self, target: &str) -> String {
        let (compilation, slice) = self.slice(target);
        slice::markdown::render(&compilation, &slice, &self.dir)
    }
}

/// Every declaration in the slice by name, with the properties it carries.
fn entries(compilation: &Compilation, slice: &Slice) -> Vec<String> {
    let store = compilation.store();
    slice
        .entries
        .iter()
        .map(|entry| match entry {
            Entry::Anchor {
                anchor, properties, ..
            } => format!("{}[{}]", store.anchor(*anchor).name, properties.join(",")),
            Entry::Variable(variable) => store.variable(*variable).name.clone(),
        })
        .collect()
}

/// Every edge as `from -relation-> to`.
fn edges(compilation: &Compilation, slice: &Slice) -> Vec<String> {
    slice
        .edges
        .iter()
        .map(|edge| {
            format!(
                "{} -{}-> {}",
                edge.from.display(compilation),
                edge.relation.as_str(),
                edge.to.display(compilation)
            )
        })
        .collect()
}

/// A small UI whose save button reads, references, and inherits across three
/// files, with an import nothing uses.
fn save_button(name: &str) -> Sandbox {
    let sandbox = Sandbox::new(name);
    sandbox
        .file(
            "components/Button.pi",
            "export abstract anchor Button as button:\n    \
             text:: string\n    \
             color:: string: gray\n    \
             click:: string\n",
        )
        .file(
            "Document.pi",
            "export anchor SaveState:\n    \
             states: [draft, saved]\n\n\
             export anchor Document:\n    \
             name: Untitled\n    \
             save: Moves @{SaveState} to saved.\n    \
             close: Discards the document.\n\n\
             export anchor Unused:\n    \
             note: nothing refers to this\n",
        )
        .file(
            "ui.pi",
            "use ./components/Button\n\
             from ./Document import Document, Unused\n\n\
             export button SaveButton:\n    \
             text: Save ${Document.name}\n    \
             color: blue\n    \
             click: Saves with @{Document.save}.\n",
        );
    sandbox
}

// -- 1. no dependencies ---------------------------------------------------

#[test]
fn a_standalone_anchor_is_its_own_slice() {
    let sandbox = Sandbox::new("standalone");
    sandbox.file(
        "main.pi",
        "export anchor Lonely:\n    note: hello\n    count: 3\n",
    );
    let (compilation, slice) = sandbox.slice("main.pi#Lonely");
    assert_eq!(entries(&compilation, &slice), ["Lonely[note,count]"]);
    assert!(slice.edges.is_empty());

    assert_eq!(
        sandbox.render("main.pi#Lonely"),
        "# Lonely\n\n\
         The part of the specification `Lonely` depends on, from `main.pi` and the \
declarations it reaches. Each declaration appears once, and the names used in \
the text refer to the sections below.\n\n\
         ## Lonely\n\n\
         Anchor in `main.pi`.\n\n\
         ### Lonely.note\n\n\
         hello\n\n\
         ### Lonely.count\n\n\
         3\n"
    );
}

// -- 2. extends -----------------------------------------------------------

#[test]
fn an_anchor_brings_in_what_it_extends() {
    let sandbox = Sandbox::new("extends");
    sandbox.file(
        "main.pi",
        "anchor Base:\n    shared: from base\n\n\
         export anchor Child extends Base:\n    own: from child\n",
    );
    let (compilation, slice) = sandbox.slice("main.pi#Child");
    assert_eq!(
        entries(&compilation, &slice),
        ["Child[shared,own]", "Base[shared]"]
    );
    assert!(edges(&compilation, &slice).contains(&"Child -inherits-> Base".to_string()));
}

// -- 3. same-file reference ----------------------------------------------

#[test]
fn a_reference_in_the_same_file_is_followed() {
    let sandbox = Sandbox::new("same-file");
    sandbox.file(
        "main.pi",
        "export anchor Guide:\n    see: Read @{Glossary} first.\n\n\
         anchor Glossary:\n    term: a word\n\n\
         anchor Elsewhere:\n    unrelated: true\n",
    );
    let (compilation, slice) = sandbox.slice("main.pi#Guide");
    assert_eq!(
        entries(&compilation, &slice),
        ["Guide[see]", "Glossary[term]"]
    );
    assert_eq!(
        edges(&compilation, &slice),
        ["Guide.see -references-> Glossary"]
    );
    let rendered = sandbox.render("main.pi#Guide");
    assert!(rendered.contains("Read Glossary first."), "{rendered}");
    assert!(!rendered.contains("Elsewhere"), "{rendered}");
}

// -- 4. across imports, and imports alone bring nothing ------------------

#[test]
fn dependencies_are_followed_across_imports_and_unused_imports_are_not() {
    let sandbox = save_button("imports");
    let (compilation, slice) = sandbox.slice("ui.pi#SaveButton");
    let names = entries(&compilation, &slice);
    assert_eq!(
        names,
        [
            "SaveButton[text,color,click]",
            "Button[text,color,click]",
            "Document[name,save]",
            "SaveState[states]",
        ]
    );
    // `ui.pi` imports Unused, but nothing depends on it.
    assert!(!names.iter().any(|name| name.starts_with("Unused")));
    // Document.close is in an imported file but nothing reads it.
    assert!(!sandbox
        .render("ui.pi#SaveButton")
        .contains("Document.close"));

    // `@{Document.save}` points at the property without reading its value.
    let edges = edges(&compilation, &slice);
    assert!(edges.contains(&"SaveButton.click -references-> Document.save".to_string()));
    assert!(!edges.contains(&"SaveButton.click -reads-> Document.save".to_string()));
    assert!(edges.contains(&"SaveButton.text -reads-> Document.name".to_string()));
}

// -- 5. transitive chain --------------------------------------------------

#[test]
fn dependencies_are_followed_transitively_nearest_first() {
    let sandbox = Sandbox::new("chain");
    sandbox.file(
        "main.pi",
        "export anchor A:\n    x: ${B.x}\n\n\
         anchor B:\n    x: see @{C}\n\n\
         anchor C:\n    body: {D}\n\n\
         anchor D:\n    leaf: end\n",
    );
    let (compilation, slice) = sandbox.slice("main.pi#A");
    assert_eq!(
        entries(&compilation, &slice),
        ["A[x]", "B[x]", "C[body]", "D[leaf]"]
    );
    assert_eq!(
        edges(&compilation, &slice),
        [
            "A.x -reads-> B.x",
            // `${B.x}` copies B.x's text, reference and all.
            "A.x -references-> C",
            "B.x -references-> C",
            "C.body -composes-> D",
        ]
    );
}

// -- 6 and 7. property targets --------------------------------------------

#[test]
fn a_property_target_is_smaller_than_its_anchor() {
    let sandbox = save_button("property");
    let (compilation, slice) = sandbox.slice("ui.pi#SaveButton.color");
    assert_eq!(
        entries(&compilation, &slice),
        ["SaveButton[color]", "Button[color]"]
    );
    assert_eq!(
        edges(&compilation, &slice),
        ["SaveButton.color -inherits-> Button.color"]
    );
}

#[test]
fn a_property_target_leaves_out_unrelated_properties() {
    let sandbox = save_button("unrelated");
    let color = sandbox.render("ui.pi#SaveButton.color");
    assert!(!color.contains("SaveButton.click"), "{color}");
    assert!(!color.contains("Document"), "{color}");
    assert!(color.contains("Only the properties this slice needs are shown."));

    // Its sibling does depend on Document.save, and so on SaveState.
    let (compilation, slice) = sandbox.slice("ui.pi#SaveButton.click");
    assert_eq!(
        entries(&compilation, &slice),
        [
            "SaveButton[click]",
            "Button[click]",
            "Document[save]",
            "SaveState[states]",
        ]
    );
}

// -- 8. shared dependencies ----------------------------------------------

#[test]
fn a_shared_dependency_is_emitted_once() {
    let sandbox = Sandbox::new("shared");
    sandbox.file(
        "main.pi",
        "export anchor Page:\n    \
         header: {Header}\n    \
         footer: {Footer}\n    \
         note: see @{Palette}\n\n\
         anchor Header:\n    tint: @{Palette}\n\n\
         anchor Footer:\n    tint: @{Palette}\n\n\
         anchor Palette:\n    primary: blue\n",
    );
    let (compilation, slice) = sandbox.slice("main.pi#Page");
    assert_eq!(
        entries(&compilation, &slice),
        [
            "Page[header,footer,note]",
            "Header[tint]",
            "Footer[tint]",
            "Palette[primary]",
        ]
    );
    let rendered = sandbox.render("main.pi#Page");
    assert_eq!(rendered.matches("## Palette\n").count(), 1, "{rendered}");
    // An embedded anchor is named, not copied, since it has its own section.
    assert!(
        rendered.contains("### Page.header\n\nHeader\n"),
        "{rendered}"
    );
}

// -- 9. cycles ------------------------------------------------------------

#[test]
fn circular_dependencies_terminate_and_stay_visible() {
    let sandbox = Sandbox::new("cycle");
    sandbox
        .file(
            "a.pi",
            "from ./b import Pong\n\nexport anchor Ping:\n    next: @{Pong}\n",
        )
        .file(
            "b.pi",
            "from ./a import Ping\n\nexport anchor Pong:\n    next: @{Ping}\n",
        );
    let (compilation, slice) = sandbox.slice("a.pi#Ping");
    assert_eq!(entries(&compilation, &slice), ["Ping[next]", "Pong[next]"]);
    // The edge that closes the cycle is kept.
    assert_eq!(
        edges(&compilation, &slice),
        [
            "Ping.next -references-> Pong",
            "Pong.next -references-> Ping"
        ]
    );

    // A cycle through inheritance's opposite direction -- a base whose value
    // refers back to its child -- terminates the same way.
    let sandbox = Sandbox::new("cycle-base");
    sandbox.file(
        "main.pi",
        "anchor Base:\n    child: @{Child}\n\n\
         export anchor Child extends Base:\n    own: x\n",
    );
    let (compilation, slice) = sandbox.slice("main.pi#Child");
    assert_eq!(
        entries(&compilation, &slice),
        ["Child[child,own]", "Base[child]"]
    );
}

// -- 10. determinism -----------------------------------------------------

#[test]
fn the_same_source_always_renders_the_same_bytes() {
    let sandbox = save_button("deterministic");
    let first = sandbox.render("ui.pi#SaveButton");
    for _ in 0..5 {
        assert_eq!(sandbox.render("ui.pi#SaveButton"), first);
    }
}

#[test]
fn reads_keep_the_order_they_are_written_in() {
    let sandbox = Sandbox::new("read-order");
    sandbox.file(
        "main.pi",
        "zeta: last\nalpha: first\n\n\
         export anchor A:\n    text: ${alpha} ${zeta}\n    other: ${zeta} ${alpha}\n",
    );
    let (compilation, slice) = sandbox.slice("main.pi#A");
    // Each property lists what it reads in the order its expression names
    // them, not in the order they are declared or were first evaluated.
    assert_eq!(
        edges(&compilation, &slice),
        [
            "A.text -reads-> alpha",
            "A.text -reads-> zeta",
            "A.other -reads-> zeta",
            "A.other -reads-> alpha",
        ]
    );
}

// -- 11 and 12. diagnostics ----------------------------------------------

#[test]
fn a_missing_anchor_is_a_diagnostic_with_a_suggestion() {
    let sandbox = save_button("missing-anchor");
    let compilation = sandbox.compile("ui.pi");
    let error = sandbox
        .resolve(&compilation, "ui.pi#SaveButon")
        .expect_err("no such anchor");
    assert_eq!(error.code, "unresolved-symbol");
    assert!(error.message.contains("`SaveButon`"), "{}", error.message);
    assert_eq!(error.help.as_deref(), Some("did you mean `SaveButton`?"));

    // A name declared in another file but not imported here is not in scope.
    let error = sandbox
        .resolve(&compilation, "ui.pi#SaveState")
        .expect_err("not imported");
    assert_eq!(error.code, "unresolved-symbol");
}

#[test]
fn a_missing_property_is_a_diagnostic_at_the_anchor() {
    let sandbox = save_button("missing-property");
    let compilation = sandbox.compile("ui.pi");
    let error = sandbox
        .resolve(&compilation, "ui.pi#SaveButton.colour")
        .expect_err("no such property");
    assert_eq!(error.code, "unknown-property");
    assert_eq!(error.message, "`SaveButton` has no property `colour`");
    assert!(error.file.ends_with("ui.pi"));
    assert_eq!(error.help.as_deref(), Some("did you mean `color`?"));
}

#[test]
fn a_path_inside_a_property_is_not_sliceable() {
    let sandbox = save_button("nested");
    let compilation = sandbox.compile("ui.pi");
    let error = sandbox
        .resolve(&compilation, "ui.pi#SaveButton.color.shade")
        .expect_err("nested path");
    assert_eq!(error.code, "unsliceable-target");
    assert_eq!(
        error.help.as_deref(),
        Some("slice `SaveButton.color` instead")
    );
}

#[test]
fn a_bare_name_declared_twice_is_ambiguous() {
    let sandbox = Sandbox::new("ambiguous");
    sandbox
        .file(
            "index.pi",
            "from ./a import Button\nfrom ./b import Card\n\n\
             export anchor Page:\n    a: {Button}\n    b: {Card}\n",
        )
        .file("a.pi", "export anchor Button:\n    color: red\n")
        .file(
            "b.pi",
            "export anchor Button:\n    color: blue\n\nexport anchor Card:\n    x: 1\n",
        );
    let compilation = sandbox.compile("index.pi");

    let error =
        slice::find(&compilation, &Target::parse("Button").unwrap()).expect_err("declared twice");
    assert_eq!(error.code, "ambiguous-target");
    assert!(error.file.ends_with("a.pi"));
    assert_eq!(error.labels.len(), 1);
    assert!(error.labels[0].file.ends_with("b.pi"));

    // Declared once, a bare name is found wherever it is.
    let entity = slice::find(&compilation, &Target::parse("Page.a").unwrap()).expect("found");
    assert_eq!(entity.display(&compilation), "Page.a");
}

#[test]
fn targets_parse_from_their_written_form() {
    let target = Target::parse("src/ui.pi#SaveButton.color").unwrap();
    assert_eq!(target.file, Some(PathBuf::from("src/ui.pi")));
    assert_eq!(target.name, "SaveButton");
    assert_eq!(target.path, ["color"]);
    assert_eq!(target.display(), "SaveButton.color");

    let bare = Target::parse("SaveButton").unwrap();
    assert_eq!(bare.file, None);
    assert!(bare.path.is_empty());

    assert!(Target::parse("ui.pi").is_err());
    assert!(Target::parse("#SaveButton").is_err());
    assert!(Target::parse("ui.pi#").is_err());
    assert!(Target::parse("ui.pi#SaveButton.").is_err());
}

// -- 13. inherited properties and constraints ----------------------------

#[test]
fn inherited_properties_and_constraints_are_included() {
    let sandbox = Sandbox::new("constraints");
    sandbox.file(
        "main.pi",
        "abstract anchor Color:\n    hex:: string\n\n\
         anchor Red extends Color:\n    hex: f00\n\n\
         abstract anchor Theme:\n    accent:: Color\n    mode:: string: light\n\n\
         export anchor Dark extends Theme:\n    accent: {Red}\n    background: black\n",
    );

    // The constraint lives on Theme, names Color, and the value embeds Red,
    // which extends Color.
    let (compilation, slice) = sandbox.slice("main.pi#Dark.accent");
    assert_eq!(
        entries(&compilation, &slice),
        ["Dark[accent]", "Theme[accent]", "Red[hex]", "Color[hex]"]
    );
    assert_eq!(
        edges(&compilation, &slice),
        [
            "Dark.accent -inherits-> Theme.accent",
            "Dark.accent -composes-> Red",
            "Theme.accent -constrains-> Color",
            "Red -inherits-> Color",
            "Red.hex -inherits-> Color.hex",
        ]
    );
    assert!(slice
        .edges
        .iter()
        .any(|edge| edge.relation == Relation::Constrains));

    // An inherited value is said to be inherited, and not written twice.
    let rendered = sandbox.render("main.pi#Dark.mode");
    assert!(
        rendered.contains(
            "### Dark.mode\n\nType: `string`. Inherited from `Theme`, with the same value.\n\n## Theme"
        ),
        "{rendered}"
    );
    assert!(
        rendered.contains("### Theme.mode\n\nType: `string`.\n\nlight\n"),
        "{rendered}"
    );
    assert!(!rendered.contains("background"), "{rendered}");

    // An abstract slot says it has no value.
    let rendered = sandbox.render("main.pi#Dark.accent");
    assert!(
        rendered.contains("### Theme.accent\n\nType: `Color`. No value; an anchor that extends this one supplies it.\n"),
        "{rendered}"
    );
}

#[test]
fn keyword_declarations_say_what_they_are() {
    let sandbox = save_button("keyword");
    let rendered = sandbox.render("ui.pi#SaveButton.color");
    assert!(
        rendered.contains(
            "## SaveButton\n\nAnchor in `ui.pi`, declared as a `button`, extending `Button`."
        ),
        "{rendered}"
    );
    assert!(
        rendered.contains(
            "## Button\n\nAbstract anchor in `components/Button.pi`, defining the keyword `button`."
        ),
        "{rendered}"
    );
}

// -- 14. expressions -----------------------------------------------------

#[test]
fn expressions_bring_in_the_variables_and_properties_they_read() {
    let sandbox = Sandbox::new("expressions");
    sandbox
        .file(
            "brand.pi",
            "export company: Acme\nexport suffix: ${company} Inc\nexport unused: never read\n",
        )
        .file(
            "main.pi",
            "from ./brand import suffix\n\n\
             export anchor Product:\n    \
             title: ${Widget.name} by ${suffix}\n    \
             price: #{Widget.cost}\n\n\
             anchor Widget:\n    name: Sprocket\n    cost: 5\n    internal: hidden\n",
        );

    let (compilation, slice) = sandbox.slice("main.pi#Product.title");
    // The value only says "Sprocket by Acme Inc"; the reads are what show it
    // came from Widget.name, suffix, and through suffix, company.
    assert_eq!(
        entries(&compilation, &slice),
        ["Product[title]", "Widget[name]", "suffix", "company"]
    );
    assert_eq!(
        edges(&compilation, &slice),
        [
            "Product.title -reads-> Widget.name",
            "Product.title -reads-> suffix",
            "suffix -reads-> company",
        ]
    );

    let rendered = sandbox.render("main.pi#Product.title");
    assert!(
        rendered.contains(
            "### Product.title\n\nReads `Widget.name`, `suffix`.\n\nSprocket by Acme Inc\n"
        ),
        "{rendered}"
    );
    assert!(
        rendered.contains("## suffix\n\nVariable in `brand.pi`. Reads `company`.\n\nAcme Inc\n"),
        "{rendered}"
    );
    assert!(!rendered.contains("internal"), "{rendered}");
    assert!(!rendered.contains("unused"), "{rendered}");
}

#[test]
fn super_and_self_reads_are_followed() {
    let sandbox = Sandbox::new("super");
    sandbox.file(
        "main.pi",
        "anchor Base:\n    description: base text\n    label: Base\n\n\
         export anchor Child extends Base:\n    \
         label: Child\n    \
         description: ${super.description} and ${self.label}\n",
    );
    let (compilation, slice) = sandbox.slice("main.pi#Child.description");
    let edges = edges(&compilation, &slice);
    assert!(edges.contains(&"Child.description -inherits-> Base.description".to_string()));
    assert!(edges.contains(&"Child.description -reads-> Base.description".to_string()));
    assert!(edges.contains(&"Child.description -reads-> Child.label".to_string()));
    // The label Child overrides is reached through Child.label.
    assert!(edges.contains(&"Child.label -inherits-> Base.label".to_string()));
}

#[test]
fn a_variable_can_be_a_target() {
    let sandbox = Sandbox::new("variable");
    sandbox.file(
        "main.pi",
        "base: 10\nexport total:: number: {base + 5}\n\nexport anchor Unrelated:\n    x: 1\n",
    );
    let (compilation, slice) = sandbox.slice("main.pi#total");
    assert_eq!(entries(&compilation, &slice), ["total", "base"]);
    let rendered = slice::markdown::render(&compilation, &slice, &sandbox.dir);
    assert!(
        rendered
            .contains("## total\n\nVariable in `main.pi`. Type: `number`. Reads `base`.\n\n15\n"),
        "{rendered}"
    );

    let compilation = sandbox.compile("main.pi");
    let error = sandbox
        .resolve(&compilation, "main.pi#total.x")
        .expect_err("variables have no properties to slice");
    assert_eq!(error.code, "unsliceable-target");
}

/// A book whose entry references a guide that embeds its chapters, next to
/// properties the chain does not pass through.
fn book(name: &str) -> Sandbox {
    let sandbox = Sandbox::new(name);
    sandbox
        .file(
            "index.pi",
            "from ./guide import Guide\n\n\
             export anchor Book:\n    \
             intro: Read this first.\n    \
             guide: @{Guide}\n",
        )
        .file(
            "guide.pi",
            "from ./chapters import Chapter, Sibling\n\n\
             export anchor Other:\n    x: 1\n\n\
             export anchor Guide:\n    \
             note: See @{Other}.\n    \
             chapters:\n        - {Sibling}\n        - {Chapter}\n",
        )
        .file(
            "chapters.pi",
            "export anchor Sibling:\n    body: Beside it.\n\n\
             export anchor Chapter:\n    body: The chapter itself.\n",
        );
    sandbox
}

#[test]
fn the_chain_from_the_entry_is_included() {
    let sandbox = book("chain");
    let compilation = sandbox.compile("index.pi");
    let entity = sandbox
        .resolve(&compilation, "chapters.pi#Chapter")
        .expect("resolves");
    let slice = slice::slice(&compilation, entity);

    let chain: Vec<String> = slice
        .chain
        .iter()
        .map(|link| link.display(&compilation))
        .collect();
    assert_eq!(chain, ["Book.guide", "Guide.chapters"]);

    // Each link is only the property on the chain, and what it refers to
    // besides the target is not followed.
    assert_eq!(
        entries(&compilation, &slice),
        ["Chapter[body]", "Book[guide]", "Guide[chapters]"]
    );

    let rendered = slice::markdown::render(&compilation, &slice, &sandbox.dir);
    assert!(
        rendered.contains(
            "The entry reaches it through `Book.guide` and `Guide.chapters`, which are included too.\n"
        ),
        "{rendered}"
    );
    assert!(!rendered.contains("Read this first."), "{rendered}");
    assert!(!rendered.contains("Beside it."), "{rendered}");
}

#[test]
fn an_export_of_the_entry_has_no_chain() {
    let sandbox = book("chain-root");
    let compilation = sandbox.compile("index.pi");
    let entity = sandbox
        .resolve(&compilation, "index.pi#Book.intro")
        .expect("resolves");
    let slice = slice::slice(&compilation, entity);
    assert!(slice.chain.is_empty());
    assert!(!slice::markdown::render(&compilation, &slice, &sandbox.dir).contains("reaches it through"));
}
