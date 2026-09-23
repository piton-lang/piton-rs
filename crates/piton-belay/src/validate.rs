//! Checks over the complete output plan, run before anything is written.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use piton_compile::{module, Compilation};
use piton_core::{is_valid_artifact_name, title_case, AnchorId, Diagnostic, Label, Span};

use crate::adapter::{Adapter, Target, FOREIGN_SKILL_ROOTS};
use crate::construct::Construct;
use crate::options::{CrossDiscovery, Options};
use crate::references::FileIndex;
use crate::{anchor_site, error_at, manifest, property_site, slash, warning_at, Draft, OutputFile, OutputKind, Planned};

/// Where a diagnostic about a whole file goes: the first anchor it carries,
/// or the configuration.
fn file_site(compilation: &Compilation, options: &Options, file: &Planned) -> (PathBuf, Span) {
    match file.anchors.first() {
        Some(anchor) => anchor_site(compilation, *anchor),
        None => (options.config.file.clone(), options.config.span),
    }
}

pub(crate) fn validate(compilation: &Compilation, options: &Options, plan: &mut Draft) {
    collisions(compilation, options, plan);
    names(compilation, options, plan);
    links(compilation, options, plan);
    descriptions(compilation, options, plan);
    cross_discovery(compilation, options, plan);

    // Nothing may escape the project root.
    for file in &plan.files {
        if file.path.is_absolute() || file.path.components().any(|c| c.as_os_str() == "..") {
            plan.diagnostics.push(
                error_at(
                    "output-outside-project",
                    format!("`{}` resolves outside the project root", file.path.display()),
                    file_site(compilation, options, file),
                )
                .with_origin(file.origin.clone()),
            );
        }
    }
}

/// Two artifacts at one path.
///
/// Within one target that is a name collision, reported even when the two
/// files would be identical: two constructs normalized to the same identity.
/// Across targets it is acceptable only when the artifacts are identical --
/// Codex and OpenCode share `AGENTS.md` -- and then the file is written once.
fn collisions(compilation: &Compilation, options: &Options, plan: &mut Draft) {
    let mut by_path: BTreeMap<PathBuf, Vec<usize>> = BTreeMap::new();
    for (index, file) in plan.files.iter().enumerate() {
        by_path.entry(file.path.clone()).or_default().push(index);
    }
    for (path, indices) in &by_path {
        if indices.len() < 2 {
            continue;
        }
        let files: Vec<&Planned> = indices.iter().map(|i| &plan.files[*i]).collect();
        let describe = |file: &Planned| format!("{} ({})", file.origin, file.target);

        let mut per_target: BTreeMap<&str, Vec<&Planned>> = BTreeMap::new();
        for file in &files {
            per_target.entry(file.target).or_default().push(file);
        }
        for (target, same) in &per_target {
            if same.len() < 2 {
                continue;
            }
            let mut diagnostic = error_at(
                "name-collision",
                format!(
                    "{} normalize to the same {} identity: `{}` would be written by {} for {target}",
                    same.iter().map(|f| format!("`{}`", f.origin)).collect::<Vec<_>>().join(" and "),
                    same[0].kind.as_str(),
                    path.display(),
                    same.iter().map(|f| describe(f)).collect::<Vec<_>>().join(", ")
                ),
                file_site(compilation, options, same[0]),
            )
            .with_origin(same[0].origin.clone())
            .with_help("rename one of them; constructs must normalize to distinct names".to_string());
            for other in &same[1..] {
                let (file, span) = file_site(compilation, options, other);
                diagnostic = diagnostic.with_label(Label::new(file, span, format!("`{}` is declared here", other.origin)));
            }
            plan.diagnostics.push(diagnostic);
        }

        let first = files[0];
        if per_target.len() > 1 && !files.iter().all(|file| file.contents == first.contents) {
            plan.diagnostics.push(
                error_at(
                    "output-collision",
                    format!(
                        "`{}` would be written with different content by: {}",
                        path.display(),
                        files.iter().map(|f| describe(f)).collect::<Vec<_>>().join(", ")
                    ),
                    file_site(compilation, options, first),
                )
                .with_origin(first.origin.clone())
                .with_help(
                    "shared output is only coalesced when the artifacts are identical in content, reference resolution, and activation behaviour"
                        .to_string(),
                ),
            );
        }
    }
    // Identical shared guidance is written once.
    let mut seen = HashSet::new();
    plan.files.retain(|file| seen.insert(file.path.clone()));
}

/// Generated names must satisfy the identity rules every target shares.
fn names(compilation: &Compilation, options: &Options, plan: &mut Draft) {
    for file in &plan.files {
        let Some(name) = artifact_name(file) else {
            continue;
        };
        if !is_valid_artifact_name(&name) {
            plan.diagnostics.push(
                error_at(
                    "invalid-artifact-name",
                    format!(
                        "`{name}` is not a valid {} name; use 1 to 64 lowercase alphanumerics with single hyphen separators",
                        file.kind.as_str()
                    ),
                    file_site(compilation, options, file),
                )
                .with_origin(file.origin.clone()),
            );
        }
        if file.kind == OutputKind::Command && !name.starts_with("x-") {
            plan.diagnostics.push(
                error_at(
                    "missing-command-prefix",
                    format!("command `{name}` lost its `x-` prefix during normalization"),
                    file_site(compilation, options, file),
                )
                .with_origin(file.origin.clone()),
            );
        }
    }
}

/// Every relative link must point at a file this plan writes, and a fragment
/// at a heading that file has.
fn links(compilation: &Compilation, options: &Options, plan: &mut Draft) {
    let planned: HashSet<String> = plan.files.iter().map(|file| slash(&file.path)).collect();
    let slugs: HashMap<String, HashSet<String>> = plan
        .files
        .iter()
        .filter(|file| file.path.extension().is_some_and(|e| e == "md"))
        .map(|file| {
            let index = FileIndex::build(&[(AnchorId(0), file.contents.as_str())]);
            (
                slash(&file.path),
                index.slugs().map(str::to_string).collect(),
            )
        })
        .collect();

    let mut broken: BTreeSet<(String, String, usize, &'static str)> = BTreeSet::new();
    for (index, file) in plan.files.iter().enumerate() {
        let directory = file.path.parent().unwrap_or(Path::new(""));
        for link in markdown_links(&file.contents) {
            if has_scheme(&link) {
                continue;
            }
            let (target, fragment) = match link.split_once('#') {
                Some((target, fragment)) => (target, Some(fragment)),
                None => (link.as_str(), None),
            };
            let key = if target.is_empty() {
                slash(&file.path)
            } else {
                slash(&module::normalize(&directory.join(target)))
            };
            if !planned.contains(&key) {
                broken.insert((slash(&file.path), link.clone(), index, "is not a generated file"));
                continue;
            }
            if let (Some(fragment), Some(headings)) = (fragment, slugs.get(&key)) {
                if !headings.contains(fragment) {
                    broken.insert((
                        slash(&file.path),
                        link.clone(),
                        index,
                        "names a heading that file does not have",
                    ));
                }
            }
        }
    }
    for (path, link, index, problem) in broken {
        let file = &plan.files[index];
        plan.diagnostics.push(
            error_at(
                "broken-reference-link",
                format!("`{path}` links to `{link}`, which {problem}"),
                file_site(compilation, options, file),
            )
            .with_origin(file.origin.clone()),
        );
    }
}

fn has_scheme(link: &str) -> bool {
    match link.split_once(':') {
        Some((scheme, _)) => {
            !scheme.is_empty()
                && !scheme.contains(['/', '#', '.'])
                && scheme.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-')
        }
        None => false,
    }
}

/// Discovery descriptions have to fit the range each target accepts.
fn descriptions(compilation: &Compilation, options: &Options, plan: &mut Draft) {
    for file in &plan.files {
        if !matches!(file.kind, OutputKind::Skill | OutputKind::Command) {
            continue;
        }
        let Some(adapter) = Adapter::by_target(file.target) else {
            continue;
        };
        let Some(description) = frontmatter_field(&file.contents, "description") else {
            continue;
        };
        let (min, max) = adapter.description_range;
        let length = description.chars().count();
        if length < min || length > max {
            let site = match file.anchors.first() {
                Some(anchor) => property_site(compilation, *anchor, "description"),
                None => file_site(compilation, options, file),
            };
            plan.diagnostics.push(
                error_at(
                    "description-too-long",
                    format!(
                        "`{}` has a {length}-character description; {} accepts {min} to {max}",
                        file.path.display(),
                        file.target
                    ),
                    site,
                )
                .with_origin(format!("{}.description", file.origin)),
            );
        }
    }
}

/// File separation alone does not guarantee target isolation: OpenCode also
/// discovers the other adapters' skill directories.
fn cross_discovery(compilation: &Compilation, options: &Options, plan: &mut Draft) {
    let config_site = || {
        (
            options.cross_discovery_site.file.clone(),
            options.cross_discovery_site.span,
        )
    };
    for discoverer in options.targets.iter().filter(|t| t.base.discovers_foreign_skills) {
        let foreign: Vec<&Planned> = plan
            .files
            .iter()
            .filter(|file| file.target != discoverer.id)
            .filter(|file| matches!(file.kind, OutputKind::Skill | OutputKind::Command))
            .filter(|file| {
                file.path.ends_with("SKILL.md")
                    && FOREIGN_SKILL_ROOTS.iter().any(|root| file.path.starts_with(root))
            })
            .collect();

        // A command translated into a skill relies on metadata or a policy
        // file OpenCode does not read, so it would be offered as an ordinary,
        // automatically selected skill.
        if options.cross_discovery.is_none() {
            for file in foreign.iter().filter(|f| f.kind == OutputKind::Command) {
                let name = artifact_name(file).unwrap_or_default();
                plan.diagnostics.push(
                    error_at(
                        "cross-discovery-activation",
                        format!(
                            "{} also discovers `{}`, which {} generates for the command `{}`; it would be offered there as an ordinary skill, changing when `{name}` activates",
                            discoverer.anchor_name,
                            file.path.display(),
                            file.target,
                            file.origin
                        ),
                        config_site(),
                    )
                    .with_origin(file.origin.clone())
                    .with_label(Label::new(
                        file_site(compilation, options, file).0,
                        file_site(compilation, options, file).1,
                        "the command is declared here",
                    ))
                    .with_help(
                        "choose a deployment in the belay-config: `crossDiscovery: separate` if each tool gets only its own tree, or `crossDiscovery: allow` to accept the change"
                            .to_string(),
                    ),
                );
            }
        }

        if options.cross_discovery == Some(CrossDiscovery::Separate) {
            continue;
        }
        let mut identities: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        let own = plan.files.iter().filter(|file| {
            file.target == discoverer.id && file.kind == OutputKind::Skill
        });
        for file in own.chain(foreign.iter().copied()) {
            if let Some(name) = artifact_name(file) {
                identities.entry(name).or_default().insert(file.target);
            }
        }
        for (name, targets) in identities.into_iter().filter(|(_, t)| t.len() > 1) {
            plan.diagnostics.push(
                warning_at(
                    "cross-target-discovery",
                    format!(
                        "skill `{name}` is generated for {}, and {} discovers the other adapters' skill directories, so it is offered more than once",
                        targets.into_iter().collect::<Vec<_>>().join(" and "),
                        discoverer.anchor_name
                    ),
                    config_site(),
                )
                .with_help(
                    "set `crossDiscovery: separate` if the trees are deployed apart, or enable fewer adapters"
                        .to_string(),
                ),
            );
        }
    }
}

/// A file at a planned path that the previous build did not record belongs to
/// someone else, and is not overwritten.
pub(crate) fn ownership(compilation: &Compilation, options: &Options, plan: &mut Draft) {
    let root = &compilation.project.root;
    let previous = manifest::previous(root);
    for file in &plan.files {
        let path = slash(&file.path);
        if previous.contains(&path) || !root.join(&file.path).exists() {
            continue;
        }
        plan.diagnostics.push(
            error_at(
                "unowned-output",
                format!(
                    "`{path}` already exists and no previous build generated it, so Belay will not overwrite it"
                ),
                file_site(compilation, options, file),
            )
            .with_origin(file.origin.clone())
            .with_help(format!(
                "move or delete `{path}` if it is safe to replace; {} lists the files Belay owns",
                manifest::MANIFEST
            )),
        );
    }
}

/// Checks scoped guidance against how the target loads it.
pub(crate) fn instruction_files(
    compilation: &Compilation,
    target: &Target,
    files: &[Planned],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let root = &compilation.project.root;
    let planned: HashMap<PathBuf, usize> = files
        .iter()
        .filter(|file| file.kind == OutputKind::Instruction)
        .map(|file| {
            (
                file.path.parent().unwrap_or(Path::new("")).to_path_buf(),
                file.contents.len(),
            )
        })
        .collect();
    let opencode_instructions = if target.base.loads_nested_instructions {
        Vec::new()
    } else {
        ["opencode.json", "opencode.jsonc"]
            .iter()
            .filter_map(|name| std::fs::read_to_string(root.join(name)).ok())
            .flat_map(|text| manifest::string_array(&text, "instructions"))
            .collect()
    };

    for file in files.iter().filter(|file| file.kind == OutputKind::Instruction) {
        let directory = file.path.parent().unwrap_or(Path::new("")).to_path_buf();
        let site = match file.anchors.first() {
            Some(anchor) => anchor_site(compilation, *anchor),
            None => (file.path.clone(), Span::default()),
        };

        if let Some(name) = target.base.instruction_override {
            let shadow = directory.join(name);
            if root.join(&shadow).is_file() {
                diagnostics.push(
                    error_at(
                        "shadowed-instruction",
                        format!(
                            "`{}` sits beside the generated `{}`, and {} reads it instead, so the generated guidance is never loaded",
                            shadow.display(),
                            file.path.display(),
                            target.id
                        ),
                        site.clone(),
                    )
                    .with_origin(file.origin.clone())
                    .with_help(format!("remove or merge `{}`", shadow.display())),
                );
            }
        }

        if let Some(limit) = target.instruction_byte_limit {
            // The target concatenates the chain from the project root down to
            // the working directory, and truncates past the limit.
            let mut total = 0u64;
            let mut chain: Vec<PathBuf> = directory.ancestors().map(Path::to_path_buf).collect();
            chain.reverse();
            for step in chain {
                let override_file = target.base.instruction_override.map(|name| root.join(&step).join(name));
                total += match override_file.filter(|path| path.is_file()) {
                    Some(path) => std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
                    None => match planned.get(&step) {
                        Some(len) => *len as u64,
                        None => std::fs::metadata(root.join(&step).join(&target.instruction_file))
                            .map(|m| m.len())
                            .unwrap_or(0),
                    },
                };
            }
            if total > limit {
                diagnostics.push(
                    error_at(
                        "instruction-over-budget",
                        format!(
                            "`{}` brings the {} instruction chain to {total} bytes, over its {limit}-byte limit, so the guidance past the limit would be dropped",
                            file.path.display(),
                            target.id
                        ),
                        site.clone(),
                    )
                    .with_origin(file.origin.clone())
                    .with_help(
                        "shorten the guidance for this scope, or raise `instructionByteLimit` in the belay-config together with Codex's project_doc_max_bytes"
                            .to_string(),
                    ),
                );
            }
        }

        if !target.base.loads_nested_instructions && !directory.as_os_str().is_empty() {
            let path = slash(&file.path);
            if !opencode_instructions.iter().any(|pattern| glob_matches(pattern, &path)) {
                diagnostics.push(
                    error_at(
                        "unguaranteed-instruction-scope",
                        format!(
                            "{} does not load `{path}` at startup unless it is configured, so the scope this guidance is placed for cannot be guaranteed",
                            target.anchor_name
                        ),
                        site.clone(),
                    )
                    .with_origin(file.origin.clone())
                    .with_help(format!(
                        "add \"{path}\" to the `instructions` array in opencode.json, or build scoped guidance only for adapters that load nested files"
                    )),
                );
            }
        }
    }
}

/// Matches a path against an `instructions` pattern: `*` and `?` stay within a
/// path segment, `**` crosses them.
fn glob_matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim_start_matches("./");
    fn go(p: &[u8], s: &[u8]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some(b'*') if p.get(1) == Some(&b'*') => {
                let rest = if p.get(2) == Some(&b'/') { &p[3..] } else { &p[2..] };
                (0..=s.len()).any(|i| go(rest, &s[i..]))
            }
            Some(b'*') => (0..=s.len())
                .take_while(|i| *i == 0 || s[i - 1] != b'/')
                .any(|i| go(&p[1..], &s[i..])),
            Some(b'?') => !s.is_empty() && s[0] != b'/' && go(&p[1..], &s[1..]),
            Some(c) => s.first() == Some(c) && go(&p[1..], &s[1..]),
        }
    }
    go(pattern.as_bytes(), path.as_bytes())
}

/// Verifies that discovery metadata and the primary body are emitted exactly
/// once: each metadata key once, each primary text once, and no primary
/// property repeated as a section of the serialized remainder.
pub(crate) fn emitted_once(
    compilation: &Compilation,
    item: &Construct,
    path: &Path,
    contents: &str,
    primary: &[&str],
    remainder_level: usize,
    plan: &mut Draft,
) {
    let mut problems = Vec::new();
    if let Some(block) = contents
        .strip_prefix("---\n")
        .and_then(|rest| rest.find("\n---").map(|end| &rest[..end]))
    {
        let mut keys = HashSet::new();
        for line in block.lines().filter(|line| !line.starts_with(' ')) {
            if let Some((key, _)) = line.split_once(':') {
                if !keys.insert(key.to_string()) {
                    problems.push(format!("metadata key `{key}` appears twice"));
                }
            }
        }
    }
    for text in primary.iter().map(|t| t.trim()).filter(|t| t.len() >= 40) {
        let count = contents.matches(text).count();
        if count != 1 {
            problems.push(format!("the primary body appears {count} times"));
        }
    }
    let primary_titles: Vec<String> = item
        .kind
        .primary_properties()
        .iter()
        .map(|name| title_case(name))
        .collect();
    for (level, title) in crate::references::scan_headings(contents) {
        if level == remainder_level && primary_titles.contains(&title) {
            problems.push(format!("`{title}` is repeated in the remainder"));
        }
    }
    for problem in problems {
        plan.diagnostics.push(
            error_at(
                "duplicated-content",
                format!("`{}`: {problem}", path.display()),
                anchor_site(compilation, item.anchor),
            )
            .with_origin(item.name.clone()),
        );
    }
}

/// The identity a skill, command, or agent file carries.
pub(crate) fn artifact_name(file: &OutputFile) -> Option<String> {
    match file.kind {
        OutputKind::Skill | OutputKind::Command => {
            if file.path.ends_with("SKILL.md") {
                file.path
                    .parent()?
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            } else {
                file.path.file_stem().map(|n| n.to_string_lossy().to_string())
            }
        }
        OutputKind::Agent => file.path.file_stem().map(|n| n.to_string_lossy().to_string()),
        _ => None,
    }
}

/// Extracts a scalar field from a YAML frontmatter block.
pub(crate) fn frontmatter_field(contents: &str, field: &str) -> Option<String> {
    let rest = contents.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    for line in rest[..end].lines() {
        if let Some(value) = line.strip_prefix(&format!("{field}: ")) {
            let value = value.trim();
            let unquoted = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            return Some(unquoted.to_string());
        }
    }
    None
}

/// Finds the targets of inline Markdown links outside fenced code.
pub(crate) fn markdown_links(contents: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in contents.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            if chars[i] == ']' && chars.get(i + 1) == Some(&'(') {
                let mut j = i + 2;
                let mut target = String::new();
                while j < chars.len() && chars[j] != ')' {
                    target.push(chars[j]);
                    j += 1;
                }
                if j < chars.len() && !target.is_empty() && !target.contains(' ') {
                    out.push(target);
                    i = j + 1;
                    continue;
                }
            }
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_links_are_found() {
        let links = markdown_links("see [A](a/b.md) and [B](../c.md#x) plus [C](https://x)\n```\n[D](d.md)\n```\n");
        assert_eq!(links, vec!["a/b.md", "../c.md#x", "https://x"]);
    }

    #[test]
    fn frontmatter_fields_are_readable() {
        let contents = "---\nname: x-thing\ndescription: \"a: b\"\n---\n\nbody";
        assert_eq!(frontmatter_field(contents, "description"), Some("a: b".to_string()));
        assert_eq!(frontmatter_field(contents, "name"), Some("x-thing".to_string()));
    }

    #[test]
    fn schemes_are_recognized() {
        assert!(has_scheme("https://x"));
        assert!(has_scheme("mailto:a@b"));
        assert!(!has_scheme("../a.md#b"));
        assert!(!has_scheme("./a.md"));
    }

    #[test]
    fn instruction_patterns_match_like_globs() {
        assert!(glob_matches("src/**/AGENTS.md", "src/components/button/AGENTS.md"));
        assert!(glob_matches("./src/AGENTS.md", "src/AGENTS.md"));
        assert!(glob_matches("src/*/AGENTS.md", "src/components/AGENTS.md"));
        assert!(!glob_matches("src/*/AGENTS.md", "src/components/button/AGENTS.md"));
    }
}
