// The Belay adapters `piton init` offers.

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
