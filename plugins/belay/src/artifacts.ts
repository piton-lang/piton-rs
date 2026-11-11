// This module renders the native artifacts of the constructs: skills,
// commands, agents and the sections of scoped guidance.

import type { Value } from "@piton/plugin";
import { optionsFor, policyPath, requiresDescription, agentPath, commandPath, skillPath, type NativeOption } from "./adapters";
import { type Construct, metadataText, remainder } from "./constructs";
import { error } from "./diagnostics";
import type { Draft } from "./draft";
import { kebabCase, titleCase } from "./names";
import { extension, fileName, join, parent } from "./paths";
import type { TargetContext } from "./plan";
import { body, discoveryDescription, roleIntroduction, type Fields } from "./render";
import { frontmatterField, tomlField } from "./scan";
import { formatNumber, hasSpace, isBlank, trim, trimEnd } from "./text";
import { emittedOnce } from "./validate";
import { checkIdentity, choose, readFile, readFolder, reportConflicts, unreadable, vendoredProperty } from "./vendored";

type Field = Fields[string];

/** A text property as Markdown for the body of `path`, with references as
 * links relative to it; null when the property is not text. */
function text(context: TargetContext, item: Construct, path: string, key: string): string | null {
  return context.space.text(path, item.anchor, key);
}

/** The properties without a place of their own, as sections from `level`. */
function remainderSections(context: TargetContext, item: Construct, path: string, consumed: string[], level: number) {
  const keys = remainder(item, consumed);
  return keys.length === 0 ? "" : context.space.sections(path, item.anchor, keys, level);
}

/** Reports a description that the target requires and the construct lacks. */
function requireDescription(context: TargetContext, item: Construct, description: string, draft: Draft): void {
  const { target } = context;
  if (!isBlank(description) || !requiresDescription(target, item.kind)) return;
  draft.diagnostics.push(
    error(
      "missing-description",
      `${target.anchorName} needs a description for the ${item.kind} \`${item.name}\`, and it has none`,
      context.world.source.anchorSite(item.anchor),
      {
        help: `add \`description:\` to \`${item.name}\`; ${target.id} discovers a ${item.kind} by it`,
      },
    ),
  );
}

/** Reports a description that the target requires and a vendored
 * definition lacks. */
function requireVendoredDescription(
  context: TargetContext,
  item: Construct,
  description: string | null,
  path: string,
  draft: Draft,
): void {
  if ((description !== null && !isBlank(description)) || !requiresDescription(context.target, item.kind)) return;
  const key = vendoredProperty(item.kind) ?? "files";
  draft.diagnostics.push(
    error(
      "missing-description",
      `${context.target.anchorName} needs a description for the ${item.kind} \`${item.name}\`, and \`${path}\` has none`,
      context.world.source.propertySite(item.anchor, key),
      { help: "add `description` to the vendored file's metadata" },
    ),
  );
}

/** Reports a field that Belay renders and the construct leaves empty. */
function requireField(context: TargetContext, item: Construct, field: string, value: string, draft: Draft): void {
  if (!isBlank(value)) return;
  draft.diagnostics.push(
    error(
      `missing-${kebabCase(field)}`,
      `the ${item.kind} \`${item.name}\` has no \`${field}\`, which Belay needs to render it`,
      context.world.source.anchorSite(item.anchor),
      { help: `add \`${field}:\` to \`${item.name}\`, or supply the whole ${item.kind} as a vendored file` },
    ),
  );
}

export function renderSkill(context: TargetContext, item: Construct, draft: Draft): void {
  const { world, target } = context;
  const { source } = world;
  const name = kebabCase(item.name);
  const path = skillPath(target, name);

  // Belay copies a vendored folder next to the SKILL.md. A SKILL.md of the
  // folder is the skill.
  const choice = choose(source, item, target, draft);
  let entries: ReturnType<typeof readFolder> = [];
  if (choice.kind === "path") {
    entries = readFolder(source, item, choice.path, draft);
    if (entries === null) return;
  } else if (choice.kind !== "render") {
    return;
  }
  const skillFile = fileName(path) ?? "";
  const folder = parent(path) ?? "";
  let verbatim: { contents: string; source: string } | null = null;
  for (const entry of entries) {
    const own = entry.relative === skillFile;
    if (own) verbatim = { contents: entry.text, source: entry.copied.source };
    draft.push({
      path: join(folder, entry.relative),
      contents: entry.text,
      kind: own ? "skill" : "skill file",
      target: target.id,
      origin: item.name,
      copied: entry.copied,
      anchors: [item.anchor],
    });
  }
  if (verbatim !== null) {
    reportConflicts(source, item, draft);
    checkIdentity(source, item, verbatim.source, frontmatterField(verbatim.contents, "name"), name, draft);
    requireVendoredDescription(context, item, frontmatterField(verbatim.contents, "description"), verbatim.source, draft);
    return;
  }

  const description = metadataText(item, "description") ?? "";
  const useWhen = metadataText(item, "useWhen") ?? "";
  const prompt = text(context, item, path, "prompt") ?? "";
  requireDescription(context, item, description, draft);
  requireField(context, item, "useWhen", useWhen, draft);

  const fields: Fields = { name };
  const discovery = discoveryDescription(description, useWhen);
  if (discovery !== "") fields.description = discovery;
  const consumed = nativeOptions(context, item, fields, draft);
  const rest = remainderSections(context, item, path, consumed, 1);
  const contents = `${source.ctx.encode("frontmatter", fields)}\n${body(prompt, rest)}`;
  emittedOnce(source, item, path, contents, [prompt], 1, draft);
  draft.push({ path, contents, kind: "skill", target: target.id, origin: item.name, copied: null, anchors: [item.anchor] });
}

export function renderCommand(context: TargetContext, item: Construct, draft: Draft): void {
  const { world, target } = context;
  const { source } = world;
  // The `x-` prefix tells a command from a skill where the two share a
  // namespace, and the normalization of target names keeps it.
  const normalized = kebabCase(item.name);
  const name = `x-${normalized}`;
  const description = metadataText(item, "description") ?? "";
  requireDescription(context, item, description, draft);

  const path = commandPath(target, normalized);
  const prompt = text(context, item, path, "prompt") ?? "";

  const fields: Fields = {};
  // A native command takes its identity from its filename, so Belay writes
  // the name only for the skill that a command becomes.
  if (target.commandSupport !== "native") fields.name = name;
  if (!isBlank(description)) fields.description = trim(description);
  // A user invokes a command deliberately. A skill that the model selects
  // would change what the command means.
  if (target.commandSupport === "translated-skill") fields["disable-model-invocation"] = true;
  const consumed = nativeOptions(context, item, fields, draft);
  const rest = remainderSections(context, item, path, consumed, 1);
  const contents = `${source.ctx.encode("frontmatter", fields)}\n${body(prompt, rest)}`;
  emittedOnce(source, item, path, contents, [prompt], 1, draft);
  draft.push({ path, contents, kind: "command", target: target.id, origin: item.name, copied: null, anchors: [item.anchor] });

  if (target.commandSupport === "translated-skill-with-policy") {
    const policy = policyPath(target, normalized);
    if (policy !== null) {
      draft.push({
        path: policy,
        contents: source.ctx.encode("yaml", { policy: { allow_implicit_invocation: false } }),
        kind: "command policy",
        target: target.id,
        origin: item.name,
        copied: null,
        anchors: [item.anchor],
      });
    }
  }
}

export function renderAgent(context: TargetContext, item: Construct, draft: Draft): void {
  const { world, target } = context;
  const { source } = world;
  const name = kebabCase(item.name);
  const path = agentPath(target, name);
  const choice = choose(source, item, target, draft);
  if (choice.kind === "path") return vendoredAgent(context, item, name, path, choice.path, draft);
  if (choice.kind !== "render") return;

  const description = metadataText(item, "description") ?? "";
  const role = text(context, item, path, "role") ?? "";
  const prompt = text(context, item, path, "prompt") ?? "";
  requireDescription(context, item, description, draft);
  requireField(context, item, "role", role, draft);

  const fields: Fields = {};
  if (target.agentFormat !== "markdown-filename-identity") fields.name = name;
  if (!isBlank(description)) fields.description = trim(description);
  if (
    target.agentFormat === "markdown-filename-identity" &&
    !item.properties.some(([key]) => key === "mode") &&
    target.defaultMode !== null
  ) {
    // Activation must be explicit. Without a mode, the tool would guess
    // whether this is a primary agent.
    fields.mode = target.defaultMode;
  }
  const consumed = nativeOptions(context, item, fields, draft);

  const introduction = roleIntroduction(role);
  const primary = isBlank(prompt) ? introduction : `${introduction}\n\n${trim(prompt)}`;
  const rest = remainderSections(context, item, path, consumed, 1);
  const assembled = body(primary, rest);

  let contents: string;
  if (target.agentFormat === "toml") {
    // The whole instruction body becomes one TOML string field.
    fields.developer_instructions = trimEnd(assembled);
    contents = source.ctx.encode("toml", fields);
  } else {
    contents = `${source.ctx.encode("frontmatter", fields)}\n${assembled}`;
    emittedOnce(source, item, path, contents, [prompt], 1, draft);
  }
  draft.push({ path, contents, kind: "agent", target: target.id, origin: item.name, copied: null, anchors: [item.anchor] });
}

/** An agent that a native file of its own defines. Belay uses the file as
 * it is. */
function vendoredAgent(context: TargetContext, item: Construct, name: string, path: string, file: string, draft: Draft): void {
  const { world, target } = context;
  const { source } = world;
  const site = source.propertySite(item.anchor, "file");
  const wanted = target.agentFormat === "toml" ? "toml" : "md";
  if (extension(file) !== wanted) {
    draft.diagnostics.push(
      error(
        "vendored-format-mismatch",
        `${target.anchorName} defines an agent in a \`.${wanted}\` file, and \`${item.name}\` names \`${file}\``,
        site,
        { help: `give \`${item.name}\` its own \`file\` for ${target.id} under \`file:\`` },
      ),
    );
    return;
  }
  const read = readFile(source, file);
  if ("error" in read) {
    draft.diagnostics.push(unreadable(source, item, file, read.error, site));
    return;
  }
  reportConflicts(source, item, draft);
  const field = target.agentFormat === "toml" ? tomlField : frontmatterField;
  // A target that takes the identity from the filename has no name to check.
  if (target.agentFormat !== "markdown-filename-identity") {
    checkIdentity(source, item, file, field(read.text, "name"), name, draft);
  }
  requireVendoredDescription(context, item, field(read.text, "description"), file, draft);
  draft.push({ path, contents: read.text, kind: "agent", target: target.id, origin: item.name, copied: read.copied, anchors: [item.anchor] });
}

/** The part of one instruction in the combined guidance of a scope. */
export interface InstructionSection {
  anchor: number;
  name: string;
  source: string;
  item: number;
  text: string;
}

/** An instruction is its title, then its description and prompt as body
 * text, then its other properties one level below its title. */
export function instructionSection(context: TargetContext, item: Construct, path: string): InstructionSection {
  const { source } = context.world;
  const primary: string[] = [];
  for (const key of ["description", "prompt"]) {
    const value = text(context, item, path, key);
    if (value !== null && !isBlank(value)) primary.push(trim(value));
  }
  const rest = remainderSections(context, item, path, [], 2);
  const content = body(primary.join("\n\n"), rest);
  const title = titleCase(item.name);
  return {
    anchor: item.anchor,
    name: item.name,
    source: source.file(item.anchor),
    item: source.anchor(item.anchor).item,
    text: content === "" ? `# ${title}\n` : `# ${title}\n\n${content}`,
  };
}

/** The names of properties that some target can carry natively. Each is
 * metadata, not prose, so it never goes into the serialized remainder. */
const NATIVE = [
  "tools",
  "model",
  "allowed-tools",
  "allowedTools",
  "mode",
  "permission",
  "agent",
  "model_reasoning_effort",
  "sandbox_mode",
];

/** Copies the native options that the construct sets into the metadata of
 * the target. An option that the target cannot represent is an error: a
 * prompt that asks for restraint does not enforce a restriction. */
function nativeOptions(context: TargetContext, item: Construct, fields: Fields, draft: Draft): string[] {
  const { world, target } = context;
  const { source } = world;
  const supported = optionsFor(target, item.kind);
  const consumed: string[] = [];
  for (const [name, value] of item.properties) {
    if (!NATIVE.includes(name)) continue;
    consumed.push(name);
    const normalized = name === "allowedTools" ? "allowed-tools" : name;
    const site = source.propertySite(item.anchor, name);
    const option = supported.find((candidate) => candidate.name === normalized);
    if (option === undefined) {
      const help =
        supported.length === 0
          ? `${target.id} has no native options for a ${item.kind}; remove \`${name}\` or build this ${item.kind} only for adapters that support it`
          : `${target.id} accepts ${supported.map((o) => `\`${o.name}\``).join(", ")} for a ${item.kind}`;
      draft.diagnostics.push(
        error(
          "unsupported-option",
          `\`${name}\` on the ${item.kind} \`${item.name}\` cannot be represented by ${target.anchorName}: it has no native \`${normalized}\` option, so it would not be enforced`,
          site,
          { help },
        ),
      );
      continue;
    }
    const result = nativeValue(option, value);
    if ("problem" in result) {
      draft.diagnostics.push(
        error("invalid-option", `\`${name}\` on \`${item.name}\` is not a valid ${target.id} value: ${result.problem}`, site),
      );
    } else {
      fields[normalized] = result.field;
    }
  }
  return consumed;
}

/** A field, or the problem when there is no value. */
function checked(value: string | null, problem: string): { field: Field } | { problem: string } {
  return value === null ? { problem } : { field: value };
}

const DECISIONS = ["allow", "ask", "deny"];

/** Plain text without its surrounding white space, or null. */
function plainText(value: Value): string | null {
  return value.kind === "text" && value.references.length === 0 ? trim(value.text) : null;
}

/** Checks the value of a native option against the rule of the target, and
 * converts it to a field. */
function nativeValue(option: NativeOption, value: Value): { field: Field } | { problem: string } {
  const rule = option.rule;
  switch (rule.kind) {
    case "text":
      return checked(plainText(value), "expected text");
    case "text-or-list": {
      if (value.kind === "list") {
        const out: string[] = [];
        for (const item of value.items) {
          if (item.kind === "text" && item.references.length === 0) out.push(item.text);
          else if (item.kind === "number") out.push(formatNumber(item.value));
          else if (item.kind === "boolean") out.push(String(item.value));
          else return { problem: "expected a list of names" };
        }
        return { field: out };
      }
      return checked(plainText(value), "expected a name or a list of names");
    }
    case "one-of": {
      const choice = plainText(value);
      return choice !== null && rule.allowed.includes(choice)
        ? { field: choice }
        : { problem: `expected one of ${rule.allowed.join(", ")}` };
    }
    case "provider-model": {
      const model = plainText(value);
      const slash = model === null ? -1 : model.indexOf("/");
      if (model !== null && slash > 0 && slash < model.length - 1 && !hasSpace(model)) return { field: model };
      return { problem: "expected a provider-qualified model such as `anthropic/claude-sonnet-4-5`" };
    }
    case "permission-map": {
      let entries: [string, Value][];
      if (value.kind === "dictionary") {
        entries = value.entries;
      } else if (value.kind === "mixed" && value.items.every((item) => item.kind === "entry")) {
        entries = [];
        for (const item of value.items) if (item.kind === "entry") entries.push([item.key, item.value]);
      } else {
        return { problem: "expected a map of tool names to allow, ask or deny" };
      }
      const out: Fields = {};
      for (const [tool, decision] of entries) {
        if (decision.kind === "dictionary") {
          const inner: Fields = {};
          for (const [pattern, choice] of decision.entries) {
            const chosen = plainText(choice);
            if (chosen === null || !DECISIONS.includes(chosen)) {
              return { problem: `\`${tool}.${pattern}\` must be allow, ask or deny` };
            }
            inner[pattern] = chosen;
          }
          out[tool] = inner;
        } else {
          const chosen = plainText(decision);
          if (chosen === null || !DECISIONS.includes(chosen)) {
            return { problem: `\`${tool}\` must be allow, ask or deny, or a map of patterns to those` };
          }
          out[tool] = chosen;
        }
      }
      return { field: out };
    }
  }
}

