// This module holds the adapters and the targets that they make.
//
// An adapter maps the four constructs onto the native files of one tool.
// The adapter anchors in the Piton files of Belay state most facts: the
// target id, the paths and the date of the last check. A project can change
// these facts in its own adapter anchor. The tables here hold the facts that
// the anchors do not state: the native options of each target, the
// constructs that need a description, and how each tool discovers files.

import type { Json } from "@piton/plugin";
import type { ConstructKind } from "./constructs";
import { trim } from "./text";

/** How a tool represents a command. */
export type CommandSupport = "translated-skill" | "translated-skill-with-policy" | "native";

/** How a tool represents an agent. */
export type AgentFormat = "markdown-named" | "markdown-filename-identity" | "toml";

/** How the value of a native option is checked before Belay writes it. */
export type OptionRule =
  | { kind: "text" }
  | { kind: "text-or-list" }
  | { kind: "one-of"; allowed: string[] }
  | { kind: "provider-model" }
  | { kind: "permission-map" };

/** One native option that a target accepts for a construct. */
export interface NativeOption {
  name: string;
  rule: OptionRule;
}

/** One output target, as Belay knows it without a compilation. */
export interface Adapter {
  targetId: string;
  exportName: string;
  root: string;
  instructionFile: string;
  referenceRoot: string;
  skillRoot: string;
  agentRoot: string;
  commandRoot: string;
  commandSupport: CommandSupport;
  agentFormat: AgentFormat;
  documentationChecked: string;
  /** Skill directories that the tool discovers and other adapters can write. */
  discoveredSkillRoots: string[];
  /** True if the tool honours the command translations of other adapters. */
  honoursCommandTranslations: boolean;
  /** True if the tool offers one copy of a skill that it finds two times. */
  offersOneCopy: boolean;
  /** False if the tool has no agent definition in a project. */
  supportsAgents: boolean;
  agentOptions: NativeOption[];
  skillOptions: NativeOption[];
  commandOptions: NativeOption[];
  descriptionRequired: ConstructKind[];
  /** The length of a discovery description that the tool accepts, in characters. */
  descriptionRange: [number, number];
  /** The byte budget of the instruction chain, if the tool cuts guidance after one. */
  instructionByteLimit: number | null;
  /** A file in the same directory that replaces the instruction file. */
  instructionOverride: string | null;
  /** False if the tool does not load nested instruction files at startup. */
  loadsNestedInstructions: boolean;
}

const text: OptionRule = { kind: "text" };
const textOrList: OptionRule = { kind: "text-or-list" };
const providerModel: OptionRule = { kind: "provider-model" };
const oneOf = (...allowed: string[]): OptionRule => ({ kind: "one-of", allowed });

const CLAUDE_TEXT_OPTIONS: NativeOption[] = [
  { name: "allowed-tools", rule: textOrList },
  { name: "model", rule: text },
];

/** Claude Code. A command becomes a skill with an `x-` prefix, and the model
 * does not invoke it. */
const CLAUDE_CODE: Adapter = {
  targetId: "claude-code",
  exportName: "ClaudeCodeAdapter",
  root: ".claude",
  instructionFile: "CLAUDE.md",
  referenceRoot: ".claude/reference",
  skillRoot: ".claude/skills",
  agentRoot: ".claude/agents",
  commandRoot: ".claude/skills",
  commandSupport: "translated-skill",
  agentFormat: "markdown-named",
  documentationChecked: "2026-09-21",
  discoveredSkillRoots: [],
  honoursCommandTranslations: false,
  offersOneCopy: false,
  supportsAgents: true,
  agentOptions: [
    { name: "tools", rule: textOrList },
    { name: "model", rule: text },
  ],
  skillOptions: CLAUDE_TEXT_OPTIONS,
  commandOptions: CLAUDE_TEXT_OPTIONS,
  descriptionRequired: ["agent"],
  descriptionRange: [1, 1024],
  instructionByteLimit: null,
  instructionOverride: null,
  loadsNestedInstructions: true,
};

/** Codex. A command becomes a skill that only the user invokes, and a policy
 * file in a separate file says so. */
const CODEX: Adapter = {
  targetId: "codex",
  exportName: "CodexAdapter",
  root: ".codex",
  instructionFile: "AGENTS.md",
  referenceRoot: ".codex/reference",
  skillRoot: ".agents/skills",
  agentRoot: ".codex/agents",
  commandRoot: ".agents/skills",
  commandSupport: "translated-skill-with-policy",
  agentFormat: "toml",
  documentationChecked: "2026-09-21",
  discoveredSkillRoots: [],
  honoursCommandTranslations: false,
  offersOneCopy: false,
  supportsAgents: true,
  agentOptions: [
    { name: "model", rule: text },
    { name: "model_reasoning_effort", rule: oneOf("minimal", "low", "medium", "high", "xhigh") },
    { name: "sandbox_mode", rule: oneOf("read-only", "workspace-write", "danger-full-access") },
  ],
  skillOptions: [],
  commandOptions: [],
  descriptionRequired: ["skill", "command", "agent"],
  descriptionRange: [1, 1024],
  // The `project_doc_max_bytes` of Codex is 32 KiB by default.
  instructionByteLimit: 32 * 1024,
  instructionOverride: "AGENTS.override.md",
  loadsNestedInstructions: true,
};

/** OpenCode. Commands are native. The tool also discovers the skill
 * directories of Claude Code and Codex. */
const OPENCODE: Adapter = {
  targetId: "opencode",
  exportName: "OpenCodeAdapter",
  root: ".opencode",
  instructionFile: "AGENTS.md",
  referenceRoot: ".opencode/reference",
  skillRoot: ".opencode/skills",
  agentRoot: ".opencode/agents",
  commandRoot: ".opencode/commands",
  commandSupport: "native",
  agentFormat: "markdown-filename-identity",
  documentationChecked: "2026-09-21",
  discoveredSkillRoots: [".claude/skills", ".agents/skills"],
  honoursCommandTranslations: false,
  offersOneCopy: false,
  supportsAgents: true,
  agentOptions: [
    { name: "mode", rule: oneOf("primary", "subagent", "all") },
    { name: "model", rule: providerModel },
    { name: "permission", rule: { kind: "permission-map" } },
  ],
  skillOptions: [],
  commandOptions: [
    { name: "agent", rule: text },
    { name: "model", rule: providerModel },
  ],
  descriptionRequired: ["skill", "agent"],
  descriptionRange: [1, 1024],
  instructionByteLimit: null,
  instructionOverride: null,
  loadsNestedInstructions: false,
};

/** Muse Code. It reads AGENTS.md and the skill directories of Codex and
 * Claude Code, and it honours their command translations. It has no agent
 * definition in a project. */
const MUSE: Adapter = {
  targetId: "muse",
  exportName: "MuseAdapter",
  root: ".muse",
  instructionFile: "AGENTS.md",
  referenceRoot: ".muse/reference",
  skillRoot: ".agents/skills",
  agentRoot: ".muse/agents",
  commandRoot: ".agents/skills",
  commandSupport: "translated-skill",
  agentFormat: "markdown-named",
  documentationChecked: "2026-10-13",
  // In the order of priority of Muse. Muse skips a copy in a later directory.
  discoveredSkillRoots: [".agents/skills", ".claude/skills", ".codex/skills"],
  honoursCommandTranslations: true,
  offersOneCopy: true,
  supportsAgents: false,
  agentOptions: [],
  skillOptions: [],
  commandOptions: [],
  descriptionRequired: ["skill", "command"],
  descriptionRange: [1, 1024],
  instructionByteLimit: null,
  instructionOverride: null,
  loadsNestedInstructions: true,
};

/** Every adapter, in the order that diagnostics list them. */
export const ADAPTERS: Adapter[] = [CLAUDE_CODE, CODEX, OPENCODE, MUSE];

/** The adapter with this target id, if one exists. */
export function adapterFor(target: string): Adapter | null {
  return ADAPTERS.find((adapter) => adapter.targetId === target) ?? null;
}

/** An adapter as the project configures it: the facts of its anchor, over
 * the facts of the adapter with the same target id. */
export interface Target {
  base: Adapter;
  id: string;
  /** The name of the adapter anchor, for diagnostics. */
  anchorName: string;
  root: string;
  instructionFile: string;
  referenceRoot: string;
  /** Output templates with a `<name>` placeholder. */
  skillOutput: string;
  commandOutput: string;
  policyOutput: string | null;
  agentOutput: string;
  commandSupport: CommandSupport;
  agentFormat: AgentFormat;
  defaultMode: string | null;
  documentationChecked: string;
  instructionByteLimit: number | null;
}

/** A target with only the facts of an adapter. */
export function builtinTarget(base: Adapter): Target {
  const command =
    base.commandSupport === "native" ? `${base.commandRoot}/x-<name>.md` : `${base.commandRoot}/x-<name>/SKILL.md`;
  return {
    base,
    id: base.targetId,
    anchorName: base.exportName,
    root: base.root,
    instructionFile: base.instructionFile,
    referenceRoot: base.referenceRoot,
    skillOutput: `${base.skillRoot}/<name>/SKILL.md`,
    commandOutput: command,
    policyOutput:
      base.commandSupport === "translated-skill-with-policy" ? `${base.commandRoot}/x-<name>/agents/openai.yaml` : null,
    agentOutput: `${base.agentRoot}/<name>.${base.agentFormat === "toml" ? "toml" : "md"}`,
    commandSupport: base.commandSupport,
    agentFormat: base.agentFormat,
    defaultMode: base.agentFormat === "markdown-filename-identity" ? "subagent" : null,
    documentationChecked: base.documentationChecked,
    instructionByteLimit: base.instructionByteLimit,
  };
}

/** Follows a dot path through dictionaries and the entries of mixed blocks. */
function lookup(properties: Record<string, Json>, path: string[]): Json | undefined {
  let value: Json | undefined = properties[path[0]];
  for (const key of path.slice(1)) {
    value = member(value, key);
    if (value === undefined) return undefined;
  }
  return value;
}

/** The value of `key` in a dictionary, or in a block that mixes keys with
 * other items. */
function member(value: Json | undefined, key: string): Json | undefined {
  if (value === null || value === undefined || typeof value !== "object") return undefined;
  if (Array.isArray(value)) {
    for (const item of value) {
      if (item !== null && typeof item === "object" && !Array.isArray(item) && Object.hasOwn(item, key)) {
        return item[key];
      }
    }
    return undefined;
  }
  return Object.hasOwn(value, key) ? value[key] : undefined;
}

/** A text value without its surrounding white space, or null. */
export function plain(value: Json | undefined): string | null {
  return typeof value === "string" ? trim(value) : null;
}

/** A target from the resolved properties of an adapter anchor. Each fact
 * that the anchor states replaces the fact of the adapter. */
export function targetFromAnchor(base: Adapter, anchorName: string, properties: Record<string, Json>): Target {
  const target = builtinTarget(base);
  target.anchorName = anchorName;
  const textAt = (...path: string[]) => plain(lookup(properties, path));
  const template = (...path: string[]) => {
    const output = textAt(...path);
    return output !== null && output.includes("<name>") ? output : null;
  };

  const file = textAt("instructionFile");
  if (file !== null) target.instructionFile = file;
  const reference = textAt("referenceRoot");
  if (reference !== null) {
    const root = reference.replace(/\/+$/, "");
    const slash = root.lastIndexOf("/");
    target.root = slash >= 0 ? root.slice(0, slash) : root;
    target.referenceRoot = root;
  }
  const checked = textAt("documentationChecked");
  if (checked !== null) target.documentationChecked = checked;
  target.skillOutput = template("skill", "output") ?? target.skillOutput;
  target.commandOutput = template("command", "output") ?? target.commandOutput;
  target.policyOutput = template("command", "policyOutput") ?? target.policyOutput;
  target.agentOutput = template("agent", "output") ?? target.agentOutput;
  const support = textAt("command", "support");
  if (support !== null) {
    target.commandSupport =
      trim(support) === "native"
        ? "native"
        : target.policyOutput !== null
          ? "translated-skill-with-policy"
          : "translated-skill";
  }
  if (target.agentOutput.endsWith(".toml")) {
    target.agentFormat = "toml";
  } else {
    const metadata = lookup(properties, ["agent", "metadata"]);
    if (metadata !== undefined) {
      target.agentFormat = hasKey(metadata, "name") ? "markdown-named" : "markdown-filename-identity";
    }
  }
  const mode = textAt("agent", "defaultMode");
  if (mode !== null) target.defaultMode = mode;
  return target;
}

function hasKey(value: Json, key: string): boolean {
  return value !== null && typeof value === "object" && member(value, key) !== undefined;
}

/** The compiled shape root of a target. */
export function shapeRoot(target: Target): string {
  return `${target.referenceRoot}/shape`;
}

export function skillPath(target: Target, name: string): string {
  return target.skillOutput.replaceAll("<name>", name);
}

/** The output path of a command, for a name without its `x-` prefix. */
export function commandPath(target: Target, name: string): string {
  return target.commandOutput.replaceAll("<name>", name);
}

export function policyPath(target: Target, name: string): string | null {
  return target.policyOutput === null ? null : target.policyOutput.replaceAll("<name>", name);
}

export function agentPath(target: Target, name: string): string {
  return target.agentOutput.replaceAll("<name>", name);
}

/** The native options that a target accepts for one construct. */
export function optionsFor(target: Target, kind: ConstructKind): NativeOption[] {
  switch (kind) {
    case "skill":
      return target.base.skillOptions;
    case "command":
      return target.base.commandOptions;
    case "agent":
      return target.base.agentOptions;
    case "instruction":
      return [];
  }
}

export function requiresDescription(target: Target, kind: ConstructKind): boolean {
  return target.base.descriptionRequired.includes(kind);
}

/** Gives true if the tool of the target discovers `path`, the SKILL.md of a
 * skill, in a directory that other adapters can write. */
export function discovers(target: Target, path: string): boolean {
  return target.base.discoveredSkillRoots.some((root) => path.startsWith(`${root}/`));
}
