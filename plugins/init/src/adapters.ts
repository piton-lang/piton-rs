// The Belay adapters `piton init` offers, and how a template's configuration
// is rewritten for the ones chosen.

import type { Context } from "@piton/plugin";

/** A Belay adapter: its target id, the anchor @piton/belay exports for it,
 * and the platform's name for people choosing. In the order @piton/belay
 * exports them. */
export interface Adapter {
  targetId: string;
  exportName: string;
  platform: string;
}

export const ADAPTERS: Adapter[] = [
  { targetId: "claude-code", exportName: "ClaudeCodeAdapter", platform: "Claude Code" },
  { targetId: "codex", exportName: "CodexAdapter", platform: "Codex" },
  { targetId: "opencode", exportName: "OpenCodeAdapter", platform: "OpenCode" },
  { targetId: "muse", exportName: "MuseAdapter", platform: "Muse Code" },
];

/** The file in a template that holds its project configuration. */
export const CONFIG = "piton.config.pi";

/** The directory in a template holding, for each adapter by target id, files
 * written only when that adapter is chosen. */
export const ADAPTER_FILES = ".adapters/";

const IMPORT = "from @piton/belay import ";

/** The adapters a template's config lists as `- {ClaudeCodeAdapter}` lines.
 * Empty when the template doesn't use Belay. */
export function listedAdapters(config: string): Adapter[] {
  return config
    .split("\n")
    .map(adapterItem)
    .filter((adapter): adapter is Adapter => adapter !== null);
}

/** The adapter a `- {ExportName}` list item names, if it names one. */
function adapterItem(line: string): Adapter | null {
  const trimmed = line.trim();
  if (!trimmed.startsWith("- {") || !trimmed.endsWith("}")) return null;
  const name = trimmed.slice(3, -1);
  return ADAPTERS.find((adapter) => adapter.exportName === name) ?? null;
}

/** The adapters `--adapter` names by target id, each once, in the order
 * @piton/belay exports them; or the first name that isn't one. */
export function adaptersNamed(names: string[]): { chosen: Adapter[] } | { unknown: string } {
  const chosen = new Set<Adapter>();
  for (const raw of names) {
    const name = raw.trim();
    const adapter = ADAPTERS.find((known) => known.targetId === name);
    if (adapter === undefined) return { unknown: name };
    chosen.add(adapter);
  }
  return { chosen: ADAPTERS.filter((adapter) => chosen.has(adapter)) };
}

/** The template's config with its adapters replaced by `chosen`: in the
 * @piton/belay import and in the `adapters` list, then formatted, which wraps
 * the import when it grows. */
export function withAdapters(ctx: Context, config: string, chosen: Adapter[]): string {
  const out: string[] = [];
  let listed = false;
  const lines = config.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  for (const line of lines) {
    if (line.startsWith(IMPORT)) {
      const names = line
        .slice(IMPORT.length)
        .split(",")
        .map((name) => name.trim())
        .filter((name) => !ADAPTERS.some((adapter) => adapter.exportName === name))
        .concat(chosen.map((adapter) => adapter.exportName));
      out.push(IMPORT + names.join(", "));
    } else if (adapterItem(line) !== null) {
      if (!listed) {
        const indent = line.slice(0, line.length - line.trimStart().length);
        for (const adapter of chosen) out.push(`${indent}- {${adapter.exportName}}`);
        listed = true;
      }
    } else {
      out.push(line);
    }
  }
  return ctx.format(out.join("\n") + "\n", CONFIG);
}
