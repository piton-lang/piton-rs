// This module reads what the project configures for Belay.
//
// The `belay-config` anchor names the adapters and makes the choices of
// deployment. The host gives Belay this anchor as its settings, with the
// properties of each adapter anchor that it names. Without settings, each
// target is the adapter with the configured target id.

import type { Json, Settings } from "@piton/plugin";
import { ADAPTERS, adapterFor, plain, targetFromAnchor, type Target } from "./adapters";
import { type Diagnostic, error } from "./diagnostics";
import type { Site, Source } from "./source";

/** The choice for skills that one adapter writes and another discovers. */
export type CrossDiscovery = "allow" | "separate";

/** The resolved configuration that Belay plans against. */
export interface Options {
  targets: Target[];
  crossDiscovery: CrossDiscovery | null;
  instructionByteLimit: number | null;
  /** The `belay-config` anchor. */
  config: Site;
  /** The `adapters` property. */
  adapters: Site;
  /** The `crossDiscovery` property, or the anchor when it has none. */
  crossDiscoverySite: Site;
  diagnostics: Diagnostic[];
}

/** True when several enabled tools read one project. Then Belay plans
 * their output together: one reference tree, and each artifact one time,
 * where the tools find it. */
export function shared(options: Options): boolean {
  return options.targets.length > 1 && options.crossDiscovery !== "separate";
}

function unknownAdapter(target: string, site: Site): Diagnostic {
  return error("unknown-adapter", `\`${target}\` is not a known Belay adapter`, site, {
    help: `available adapters: ${ADAPTERS.map((a) => `${a.exportName} (${a.targetId})`).join(", ")}`,
  });
}

interface Configured {
  name: string;
  targetId: string;
  properties: Record<string, Json>;
  site: Site;
}

/** Reads the options of the project. */
export function loadOptions(source: Source): Options {
  const project = source.project;
  const fallback: Site = { file: project.configPath ?? project.root, start: 0, end: 0 };
  const options: Options = {
    targets: [],
    crossDiscovery: null,
    instructionByteLimit: null,
    config: fallback,
    adapters: fallback,
    crossDiscoverySite: fallback,
    diagnostics: [],
  };

  const settings = project.configPath !== null && source.isFile(project.configPath) ? source.ctx.settings() : null;
  if (settings !== null) {
    for (const adapter of readSettings(settings, options)) {
      const base = adapterFor(adapter.targetId);
      if (base === null) options.diagnostics.push(unknownAdapter(adapter.targetId, adapter.site));
      else options.targets.push(targetFromAnchor(base, adapter.name, adapter.properties));
    }
  }

  if (options.instructionByteLimit !== null) {
    for (const target of options.targets) {
      if (target.instructionByteLimit !== null) target.instructionByteLimit = options.instructionByteLimit;
    }
  }
  return options;
}

/** The names of the anchors that a value lists, in order. */
function listedAnchors(value: Json | undefined): string[] {
  const named = (item: Json): string | null =>
    item !== null && typeof item === "object" && !Array.isArray(item) && typeof item.$anchor === "string"
      ? item.$anchor
      : null;
  if (value === undefined || value === null) return [];
  const items = Array.isArray(value) ? value : [value];
  return items.map(named).filter((name): name is string => name !== null);
}

/** Reads the settings, and gives the adapters that they configure. */
function readSettings(settings: Settings, options: Options): Configured[] {
  const properties = settings.properties;
  const site = (key: string): Site => settings.location(key);
  options.config = settings.location();
  options.adapters = site("adapters");
  options.crossDiscoverySite = site("crossDiscovery");

  const cross = properties.crossDiscovery;
  if (cross !== undefined && cross !== null) {
    const choice = plain(cross);
    if (choice === "allow" || choice === "separate") {
      options.crossDiscovery = choice;
    } else {
      options.diagnostics.push(
        error("invalid-cross-discovery", "`crossDiscovery` must be `allow` or `separate`", site("crossDiscovery")),
      );
    }
  }
  const limit = properties.instructionByteLimit;
  if (limit !== undefined && limit !== null) {
    if (typeof limit === "number" && limit >= 1 && Number.isInteger(limit)) {
      options.instructionByteLimit = limit;
    } else {
      options.diagnostics.push(
        error(
          "invalid-instruction-byte-limit",
          "`instructionByteLimit` must be a positive whole number of bytes",
          site("instructionByteLimit"),
        ),
      );
    }
  }

  const adapters: Configured[] = [];
  for (const name of listedAnchors(properties.adapters)) {
    const adapterProperties = settings.anchors[name] ?? {};
    const targetId = plain(adapterProperties.targetId);
    const at = settings.anchorLocations[name] ?? settings.location();
    // An adapter without a targetId names no target, so Belay builds nothing
    // for it.
    if (targetId === null) {
      options.diagnostics.push(error("invalid-adapter", `\`${name}\` has no targetId`, at));
      continue;
    }
    adapters.push({ name, targetId, properties: adapterProperties, site: at });
  }
  return adapters;
}
