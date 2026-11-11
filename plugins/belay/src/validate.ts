// This module checks the complete plan before the host writes it.

import { adapterFor, discovers, type Target } from "./adapters";
import { primaryProperties, type Construct } from "./constructs";
import { type Diagnostic, error, warning } from "./diagnostics";
import { data, type Draft, type Planned } from "./draft";
import { titleCase, isValidArtifactName } from "./names";
import { type Options, shared } from "./options";
import {
  ancestors,
  clean,
  directory,
  endsWith,
  extension,
  fileName,
  fileStem,
  isAbsolute,
  join,
  leavesUpward,
  normalize,
  parent,
} from "./paths";
import { frontmatterField, globMatches, hasScheme, markdownLinks, scanHeadings, slugs, stringArray } from "./scan";
import type { Site, Source } from "./source";
import { byteLength, charCount, compareText, lines, occurrences, trim, trimStart } from "./text";

/** Where the build records the files that it owns. */
export const MANIFEST = ".piton/manifest.json";

/** Where a diagnostic about a whole file goes: the first anchor in it, or
 * the configuration. */
function fileSite(source: Source, options: Options, file: Planned): Site {
  return file.anchors.length > 0 ? source.anchorSite(file.anchors[0]) : options.config;
}

/** Runs the checks of the whole plan. */
export function validate(source: Source, options: Options, draft: Draft): void {
  collisions(source, options, draft);
  names(source, options, draft);
  links(source, options, draft);
  descriptions(source, options, draft);
  crossDiscovery(source, options, draft);

  // No file may leave the project root.
  for (const file of draft.files) {
    if (isAbsolute(file.path) || leavesUpward(file.path)) {
      draft.diagnostics.push(
        error("output-outside-project", `\`${file.path}\` resolves outside the project root`, fileSite(source, options, file)),
      );
    }
  }
}

/** Two artifacts at one path.
 *
 * In one target, that is a collision of names, even when the two files are
 * the same: two constructs have one identity. Across targets, it is
 * acceptable when the artifacts are the same, and Belay writes the file one
 * time. In a shared project, the first target writes an instruction file
 * that several targets place, and the other tools read that file. */
function collisions(source: Source, options: Options, draft: Draft): void {
  const byPath = new Map<string, Planned[]>();
  for (const file of draft.files) {
    const key = clean(file.path);
    const group = byPath.get(key);
    if (group === undefined) byPath.set(key, [file]);
    else group.push(file);
  }
  for (const files of byPath.values()) {
    if (files.length < 2) continue;
    const path = files[0].path;
    const describe = (file: Planned) => `${file.origin} (${file.target})`;
    const perTarget = new Map<string, Planned[]>();
    for (const file of files) {
      const group = perTarget.get(file.target);
      if (group === undefined) perTarget.set(file.target, [file]);
      else group.push(file);
    }
    for (const [target, same] of perTarget) {
      if (same.length < 2) continue;
      draft.diagnostics.push(
        error(
          "name-collision",
          `${same.map((f) => `\`${f.origin}\``).join(" and ")} normalize to the same ${same[0].kind} identity: \`${path}\` would be written by ${same.map(describe).join(", ")} for ${target}`,
          fileSite(source, options, same[0]),
          { help: "rename one of them; constructs must normalize to distinct names" },
        ),
      );
    }
    const first = files[0];
    const sharedInstruction = shared(options) && files.every((file) => file.kind === "instruction");
    if (perTarget.size > 1 && !sharedInstruction && !files.every((file) => data(file) === data(first))) {
      draft.diagnostics.push(
        error(
          "output-collision",
          `\`${path}\` would be written with different content by: ${files.map(describe).join(", ")}`,
          fileSite(source, options, first),
          {
            help: "shared output is only coalesced when the artifacts are identical in content, reference resolution, and activation behaviour",
          },
        ),
      );
    }
  }
  // Belay writes shared guidance one time. The files are in the order of the
  // targets, so the copy of the first target stays.
  const seen = new Set<string>();
  draft.files = draft.files.filter((file) => {
    const key = clean(file.path);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/** The identity that a skill, command or agent file has. */
export function artifactName(file: Planned): string | null {
  switch (file.kind) {
    case "skill":
    case "command":
      if (endsWith(file.path, "SKILL.md")) {
        const above = parent(file.path);
        return above === null ? null : fileName(above);
      }
      return fileStem(file.path);
    case "agent":
      return fileStem(file.path);
    default:
      return null;
  }
}

/** Generated names must obey the identity rules that all targets share. */
function names(source: Source, options: Options, draft: Draft): void {
  for (const file of draft.files) {
    const name = artifactName(file);
    if (name === null) continue;
    if (!isValidArtifactName(name)) {
      draft.diagnostics.push(
        error(
          "invalid-artifact-name",
          `\`${name}\` is not a valid ${file.kind} name; use 1 to 64 lowercase alphanumerics with single hyphen separators`,
          fileSite(source, options, file),
        ),
      );
    }
    if (file.kind === "command" && !name.startsWith("x-")) {
      draft.diagnostics.push(
        error(
          "missing-command-prefix",
          `command \`${name}\` lost its \`x-\` prefix during normalization`,
          fileSite(source, options, file),
        ),
      );
    }
  }
}

/** Each relative link that Belay wrote must point at a file of the plan, and
 * a fragment must name a heading of that file. */
function links(source: Source, options: Options, draft: Draft): void {
  const planned = new Set(draft.files.map((file) => file.path));
  const fragments = new Map<string, Set<string>>();
  for (const file of draft.files) {
    if (extension(file.path) === "md") fragments.set(file.path, slugs(file.contents));
  }
  const broken = new Map<string, [string, string, number, string]>();
  draft.files.forEach((file, index) => {
    const from = directory(file.path);
    for (const link of markdownLinks(file.contents)) {
      // Only the links that Belay wrote must land on its output. Belay
      // passes a link that an author wrote through as it is.
      if (hasScheme(link) || !draft.wroteLink(from, link)) continue;
      const hash = link.indexOf("#");
      const target = hash < 0 ? link : link.slice(0, hash);
      const fragment = hash < 0 ? null : link.slice(hash + 1);
      const key = target === "" ? file.path : normalize(join(from, target));
      const add = (problem: string) => {
        const entry: [string, string, number, string] = [file.path, link, index, problem];
        broken.set(JSON.stringify(entry), entry);
      };
      if (!planned.has(key)) {
        add("is not a generated file");
        continue;
      }
      const headings = fragments.get(key);
      if (fragment !== null && headings !== undefined && !headings.has(fragment)) {
        add("names a heading that file does not have");
      }
    }
  });
  for (const [path, link, index, problem] of broken.values()) {
    draft.diagnostics.push(
      error(
        "broken-reference-link",
        `\`${path}\` links to \`${link}\`, which ${problem}`,
        fileSite(source, options, draft.files[index]),
      ),
    );
  }
}

/** A discovery description must fit the range that each target accepts. */
function descriptions(source: Source, options: Options, draft: Draft): void {
  for (const file of draft.files) {
    if (file.kind !== "skill" && file.kind !== "command") continue;
    const adapter = adapterFor(file.target);
    if (adapter === null) continue;
    const description = frontmatterField(file.contents, "description");
    if (description === null) continue;
    const [min, max] = adapter.descriptionRange;
    const length = charCount(description);
    if (length >= min && length <= max) continue;
    const site =
      file.anchors.length > 0 ? source.propertySite(file.anchors[0], "description") : fileSite(source, options, file);
    draft.diagnostics.push(
      error(
        "description-too-long",
        `\`${file.path}\` has a ${length}-character description; ${file.target} accepts ${min} to ${max}`,
        site,
      ),
    );
  }
}

/** The skill permissions of OpenCode, from `opencode.json` or
 * `opencode.jsonc`, in order: each name pattern with `allow`, `ask` or
 * `deny`. One action for each skill is the pattern `*`. */
function opencodeSkillPermissions(source: Source): [string, string][] {
  for (const name of ["opencode.json", "opencode.jsonc"]) {
    const text = source.readText(join(source.project.root, name));
    if (text === null) continue;
    // Belay drops the line comments of JSONC. It reads the rest as JSON, and
    // a file that it cannot parse grants nothing.
    const json = lines(text)
      .filter((line) => !trimStart(line).startsWith("//"))
      .join("\n");
    let document: unknown;
    try {
      document = JSON.parse(json);
    } catch {
      continue;
    }
    const member = (value: unknown, key: string): unknown =>
      value !== null && typeof value === "object" && !Array.isArray(value) && Object.hasOwn(value, key)
        ? (value as Record<string, unknown>)[key]
        : undefined;
    const skill = member(member(document, "permission"), "skill");
    if (typeof skill === "string") return [["*", skill]];
    if (skill !== null && typeof skill === "object" && !Array.isArray(skill)) {
      return Object.entries(skill as Record<string, unknown>)
        .filter((entry): entry is [string, string] => typeof entry[1] === "string")
        .map(([pattern, action]) => [pattern, action]);
    }
    return [];
  }
  return [];
}

/** Gives true if OpenCode hides the skill `name`. The last pattern that
 * matches the name decides. */
function denied(permissions: [string, string][], name: string): boolean {
  for (let i = permissions.length - 1; i >= 0; i -= 1) {
    const [pattern, action] = permissions[i];
    if (globMatches(pattern, name)) return action === "deny";
  }
  return false;
}

/** Separate files do not isolate the targets: OpenCode and Muse also
 * discover the skill directories of other adapters. */
function crossDiscovery(source: Source, options: Options, draft: Draft): void {
  const configSite = options.crossDiscoverySite;
  const permissions = opencodeSkillPermissions(source);
  for (const discoverer of options.targets.filter((t) => t.base.discoveredSkillRoots.length > 0)) {
    const foreign = draft.files
      .filter((file) => file.target !== discoverer.id)
      .filter((file) => file.kind === "skill" || file.kind === "command")
      .filter((file) => endsWith(file.path, "SKILL.md") && discovers(discoverer, file.path))
      // The tool does not offer a skill that it is told to hide.
      .filter((file) => !denied(permissions, artifactName(file) ?? ""));

    // A command that becomes a skill gets its activation from metadata or a
    // policy file. A tool that does not honour them offers the command as an
    // ordinary skill that the model selects.
    if (options.crossDiscovery === null && !discoverer.base.honoursCommandTranslations) {
      for (const file of foreign.filter((f) => f.kind === "command")) {
        const name = artifactName(file) ?? "";
        draft.diagnostics.push(
          error(
            "cross-discovery-activation",
            `${discoverer.anchorName} also discovers \`${file.path}\`, which ${file.target} generates for the command \`${file.origin}\`; it would be offered there as an ordinary skill, changing when \`${name}\` activates`,
            configSite,
            {
              help: 'hide the command skills from OpenCode with `"permission": {"skill": {"x-*": "deny"}}` in opencode.json; or set `crossDiscovery: separate` in the belay-config if each tool gets only its own tree, or `crossDiscovery: allow` to accept the change',
            },
          ),
        );
      }
    }

    // A tool that keeps one copy of each skill offers no skill two times.
    if (options.crossDiscovery === "separate" || discoverer.base.offersOneCopy) continue;
    const identities = new Map<string, Set<string>>();
    const own = draft.files.filter((file) => file.target === discoverer.id && file.kind === "skill");
    for (const file of [...own, ...foreign]) {
      const name = artifactName(file);
      if (name === null) continue;
      let directories = identities.get(name);
      if (directories === undefined) identities.set(name, (directories = new Set()));
      directories.add(directory(file.path));
    }
    const sorted = [...identities.entries()].sort(([a], [b]) => compareText(a, b));
    for (const [name, directories] of sorted) {
      if (directories.size < 2) continue;
      const listed = [...directories].sort(compareText).map((d) => `\`${d}\``);
      draft.diagnostics.push(
        warning(
          "cross-target-discovery",
          `${discoverer.anchorName} discovers the skill \`${name}\` in ${listed.join(" and ")}, so it offers it more than once`,
          configSite,
          {
            help: "nothing in the project stops OpenCode reading both; set OPENCODE_DISABLE_CLAUDE_CODE_SKILLS=1 where it runs, set `crossDiscovery: separate` if the trees are deployed apart, or enable fewer adapters",
          },
        ),
      );
    }
  }
}

/** A file at a planned path that the last build did not record belongs to
 * someone else. Belay does not overwrite it. */
export function ownership(source: Source, options: Options, draft: Draft): void {
  const root = source.project.root;
  const previous = new Set(stringArray(source.readText(join(root, MANIFEST)) ?? "", "generated"));
  for (const file of draft.files) {
    if (previous.has(file.path) || source.stat(join(root, file.path)) === null) continue;
    draft.diagnostics.push(
      error(
        "unowned-output",
        `\`${file.path}\` already exists and no previous build generated it, so Belay will not overwrite it`,
        fileSite(source, options, file),
        { help: `move or delete \`${file.path}\` if it is safe to replace; ${MANIFEST} lists the files Belay owns` },
      ),
    );
  }
}

/** Checks scoped guidance against how the target loads it. */
export function instructionFiles(source: Source, target: Target, files: Planned[], diagnostics: Diagnostic[]): void {
  const root = source.project.root;
  const instructions = files.filter((file) => file.kind === "instruction");
  const planned = new Map<string, number>();
  for (const file of instructions) planned.set(clean(directory(file.path)), byteLength(file.contents));
  const opencodeInstructions: string[] = [];
  if (!target.base.loadsNestedInstructions) {
    for (const name of ["opencode.json", "opencode.jsonc"]) {
      const text = source.readText(join(root, name));
      if (text !== null) opencodeInstructions.push(...stringArray(text, "instructions"));
    }
  }

  for (const file of instructions) {
    const scope = directory(file.path);
    const site: Site = file.anchors.length > 0 ? source.anchorSite(file.anchors[0]) : { file: file.path, start: 0, end: 0 };

    const override = target.base.instructionOverride;
    if (override !== null) {
      const shadow = join(scope, override);
      if (source.isFile(join(root, shadow))) {
        diagnostics.push(
          error(
            "shadowed-instruction",
            `\`${shadow}\` sits beside the generated \`${file.path}\`, and ${target.id} reads it instead, so the generated guidance is never loaded`,
            site,
            { help: `remove or merge \`${shadow}\`` },
          ),
        );
      }
    }

    const limit = target.instructionByteLimit;
    if (limit !== null) {
      // The tool joins the chain from the project root down to the working
      // directory, and cuts it after the limit.
      let total = 0;
      for (const step of ancestors(scope).reverse()) {
        const overrideFile = override === null ? null : join(join(root, step), override);
        if (overrideFile !== null && source.isFile(overrideFile)) {
          total += source.stat(overrideFile)?.size ?? 0;
        } else {
          const size = planned.get(clean(step));
          total += size ?? source.stat(join(join(root, step), target.instructionFile))?.size ?? 0;
        }
      }
      if (total > limit) {
        diagnostics.push(
          error(
            "instruction-over-budget",
            `\`${file.path}\` brings the ${target.id} instruction chain to ${total} bytes, over its ${limit}-byte limit, so the guidance past the limit would be dropped`,
            site,
            {
              help: "shorten the guidance for this scope, or raise `instructionByteLimit` in the belay-config together with Codex's project_doc_max_bytes",
            },
          ),
        );
      }
    }

    if (!target.base.loadsNestedInstructions && scope !== "") {
      if (!opencodeInstructions.some((pattern) => globMatches(pattern, file.path))) {
        diagnostics.push(
          error(
            "unguaranteed-instruction-scope",
            `${target.anchorName} does not load \`${file.path}\` at startup unless it is configured, so the scope this guidance is placed for cannot be guaranteed`,
            site,
            {
              help: `add "${file.path}" to the \`instructions\` array in opencode.json, or build scoped guidance only for adapters that load nested files`,
            },
          ),
        );
      }
    }
  }
}

/** Checks that discovery metadata and the primary body appear one time
 * each: each metadata key, each primary text, and no primary property again
 * as a section of the remainder. */
export function emittedOnce(
  source: Source,
  item: Construct,
  path: string,
  contents: string,
  primary: string[],
  remainderLevel: number,
  draft: Draft,
): void {
  const problems: string[] = [];
  if (contents.startsWith("---\n")) {
    const rest = contents.slice(4);
    const end = rest.indexOf("\n---");
    if (end >= 0) {
      const keys = new Set<string>();
      for (const line of lines(rest.slice(0, end))) {
        if (line.startsWith(" ")) continue;
        const colon = line.indexOf(":");
        if (colon < 0) continue;
        const key = line.slice(0, colon);
        if (keys.has(key)) problems.push(`metadata key \`${key}\` appears twice`);
        keys.add(key);
      }
    }
  }
  for (const text of primary.map(trim).filter((t) => byteLength(t) >= 40)) {
    const count = occurrences(contents, text);
    if (count !== 1) problems.push(`the primary body appears ${count} times`);
  }
  const titles = primaryProperties(item.kind).map(titleCase);
  for (const [level, title] of scanHeadings(contents)) {
    if (level === remainderLevel && titles.includes(title)) problems.push(`\`${title}\` is repeated in the remainder`);
  }
  for (const problem of problems) {
    draft.diagnostics.push(error("duplicated-content", `\`${path}\`: ${problem}`, source.anchorSite(item.anchor)));
  }
}
