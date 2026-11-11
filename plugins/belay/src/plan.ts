// This module plans the output of Belay for each target.
//
// Belay emits what the entry file exports, and what those exports reference.
// An exported construct becomes the native artifact of its target. Any other
// exported anchor, and each anchor that an emitted artifact references, goes
// into the reference tree of the target, one file for each source module.
//
// Belay plans all targets first, then checks the complete plan. Thus it finds
// collisions, broken links, files that it does not own and problems between
// targets before the host writes anything.

import type { Layout as LinkLayout, LinkSpace } from "@piton/plugin";
import {
  agentPath,
  commandPath,
  discovers,
  shapeRoot,
  skillPath,
  type Target,
} from "./adapters";
import { type Construct, Constructs, metadataProperties } from "./constructs";
import { error, warning } from "./diagnostics";
import { Draft } from "./draft";
import { instructionSection, renderAgent, renderCommand, renderSkill, type InstructionSection } from "./artifacts";
import { kebabCase } from "./names";
import { type Options, shared } from "./options";
import { comparePaths, join, normalize, startsWith, underRoot } from "./paths";
import { type Closure, close, display, documentPath } from "./references";
import { isShapeSource, place } from "./shape";
import type { Source } from "./source";
import { trimEnd } from "./text";
import { instructionFiles, ownership, validate } from "./validate";
import { checkTargets, reportUnused } from "./vendored";

/** The exports of the entry, sorted by what they compile to. */
export interface Emission {
  /** Exported concrete constructs, which become native artifacts. */
  constructs: Construct[];
  /** Exported concrete anchors that are not constructs, which become
   * reference documents. */
  documents: number[];
  /** Exported instructions under `shapeRoot`. The compiled shape tree keeps
   * them, and they are also placed as scoped guidance. */
  shapeInstructions: Set<number>;
  /** The scope directory of each placeable instruction, relative to the
   * project root. */
  placements: Map<number, string>;
}

/** What Belay needs while it plans one project. */
export interface World {
  source: Source;
  constructs: Constructs;
  options: Options;
  /** The marker of each placeholder, by name. */
  markers: Record<string, string>;
}

/** Plans the project. */
export function plan(world: World): Draft {
  const { source, options } = world;
  const draft = new Draft();
  draft.diagnostics.push(...options.diagnostics);
  if (options.targets.length === 0) return draft;

  const emission = collectEmission(world, draft);
  const closure = close(source, roots(emission));
  reportReferences(world, emission, closure, draft);
  for (const item of emission.constructs) checkTargets(source, item, draft);

  const targets = plannedTargets(options);
  const sharing = new Sharing(shared(options), targets);
  for (const target of targets) buildTarget(world, target, sharing, emission, closure, draft);

  validate(source, options, draft);
  ownership(source, options, draft);
  draft.files.sort((a, b) => comparePaths(a.path, b.path) || (a.target < b.target ? -1 : a.target > b.target ? 1 : 0));
  return draft;
}

function collectEmission(world: World, draft: Draft): Emission {
  const { source } = world;
  const exports = source.compilation.exports().filter((anchor) => !source.anchor(anchor).abstract);
  const constructs = world.constructs.collect(source, exports);
  const constructIds = new Set(constructs.map((item) => item.anchor));
  const documents: number[] = [];
  for (const anchor of exports) {
    if (!constructIds.has(anchor) && !documents.includes(anchor)) documents.push(anchor);
  }

  const projectRoot = source.project.root;
  const configuredShape = source.project.shapeRoot;
  const codeRoot = source.project.codeRoot;
  const shapeInstructions = new Set<number>();
  const placements = new Map<number, string>();
  for (const item of constructs.filter((c) => c.kind === "instruction")) {
    const file = source.file(item.anchor);
    const site = source.anchorSite(item.anchor);
    if (configuredShape === null || !isShapeSource(file, configuredShape)) {
      // Where an instruction outside shapeRoot goes is an open decision of
      // the specification. Belay says so, and does not guess.
      draft.diagnostics.push(
        error(
          "instruction-placement-unspecified",
          `\`${item.name}\` is not under the configured shapeRoot, and where such an instruction goes is not specified`,
          site,
          {
            help: "move it under shapeRoot (and configure one), or make it a skill; see OpenDecisions.instructionScope",
          },
        ),
      );
      continue;
    }
    shapeInstructions.add(item.anchor);
    const scope = place(file, configuredShape, codeRoot, (path) => source.isDirectory(path));
    if (scope === null) {
      draft.diagnostics.push(
        error(
          "unplaceable-instruction",
          `\`${item.name}\` has no scope to attach to; \`${codeRoot}\` does not exist`,
          site,
        ),
      );
    } else {
      placements.set(item.anchor, underRoot(scope, projectRoot));
    }
  }
  return { constructs, documents, shapeInstructions, placements };
}

/** The anchors whose content Belay emits directly. */
function roots(emission: Emission): number[] {
  return [...emission.constructs.map((c) => c.anchor), ...emission.documents];
}

/** Reports references that Belay cannot compile, and references in
 * metadata. */
function reportReferences(world: World, emission: Emission, closure: Closure, draft: Draft): void {
  const { source } = world;
  const reported = new Set<string>();
  for (const occurrence of closure.occurrences) {
    const target = occurrence.target;
    const key = JSON.stringify([target.anchor, occurrence.from, occurrence.key]);
    if (reported.has(key)) continue;
    reported.add(key);
    const reason = closure.unrepresentable.get(target.anchor);
    if (reason === undefined) continue;
    const why =
      reason === "abstract"
        ? "an abstract anchor, which is never compiled"
        : reason === "package"
          ? `\`${source.name(target.anchor)}\` from a bundled package, which has no compiled representation in this project`
          : "an anchor outside the project, which no output location covers";
    draft.diagnostics.push(
      error(
        "unrepresentable-reference",
        `\`@{${display(source, target)}}\` points at ${why}, so the link would have nowhere to go`,
        source.propertySite(occurrence.from, occurrence.key),
        { help: "reference a concrete anchor of this project, or write the name as text" },
      ),
    );
  }

  // Discovery metadata is plain text: a reader would not read a link there.
  for (const item of emission.constructs) {
    for (const key of metadataProperties(item.kind)) {
      const value = item.properties.find(([name]) => name === key)?.[1];
      if (value?.kind !== "text" || value.references.length === 0) continue;
      draft.diagnostics.push(
        error(
          "unrepresentable-reference",
          `\`${item.name}.${key}\` is emitted as target metadata, which has no representation for a reference`,
          source.propertySite(item.anchor, key),
          { help: "write the name as plain text, or move the reference into the prompt" },
        ),
      );
    }
  }
}

/** The enabled targets as Belay plans them. In a shared project, each
 * target links into the reference tree of the first, so the files that the
 * tools share, like AGENTS.md, are the same. */
export function plannedTargets(options: Options): Target[] {
  const isShared = shared(options);
  return options.targets.map((target) =>
    isShared ? { ...target, referenceRoot: options.targets[0].referenceRoot } : { ...target },
  );
}

/** How the enabled targets share one project. */
export class Sharing {
  constructor(
    /** False with one target, or when Belay deploys the trees apart. */
    readonly shared: boolean,
    /** Each enabled target, in the configured order, as planned. */
    readonly targets: Target[],
  ) {}

  /** Gives true if `target` writes the reference tree. */
  ownsReferences(target: Target): boolean {
    return !this.shared || (this.targets.length > 0 && this.targets[0].id === target.id);
  }

  /** The enabled target whose skills the tool of `target` already
   * discovers. Then `target` writes no copies, and links to those. */
  skillSource(target: Target): Target | null {
    if (!this.shared) return null;
    return this.targets.find((peer) => peer.id !== target.id && discovers(target, peer.skillOutput)) ?? null;
  }

  /** The enabled target whose command skills the tool of `target`
   * discovers and honours as commands. Then `target` writes none. */
  commandSource(target: Target): Target | null {
    if (!this.shared || !target.base.honoursCommandTranslations) return null;
    return (
      this.targets.find(
        (peer) => peer.id !== target.id && peer.commandSupport !== "native" && discovers(target, peer.commandOutput),
      ) ?? null
    );
  }

  /** For a target without agent definitions, the enabled target whose agent
   * files a link to an agent goes to. */
  agentSource(target: Target): Target | null {
    if (!this.shared || target.base.supportsAgents) return null;
    return this.targets.find((peer) => peer.id !== target.id && peer.base.supportsAgents) ?? null;
  }
}

/** What rendering needs for one target. */
export interface TargetContext {
  world: World;
  target: Target;
  space: LinkSpace;
}

/** Where one target puts each document and native artifact. */
interface Layout {
  context: TargetContext;
  /** Each reference document and the anchors in it, in order, sorted by path. */
  files: [string, number[]][];
  compiledShape: string;
  /** Where the target puts each anchor, as the host reads it. */
  spec: LinkLayout;
}

/** The properties that a construct leaves out of its reference document
 * when they are null. */
function omitted(world: World, anchor: number): string[] {
  return world.constructs.classify(anchor) !== null ? ["description", "prompt"] : [];
}

function layout(world: World, target: Target, sharing: Sharing, emission: Emission, closure: Closure): Layout {
  const { source } = world;
  const project = source.project;

  // Belay needs the destinations of references before it renders anything,
  // because a link must point at a planned output. A reference to a skill,
  // command or agent links to the output of that construct.
  const natives = new Map<number, string>();
  for (const item of emission.constructs) {
    const name = kebabCase(item.name);
    let path: string;
    if (item.kind === "skill") {
      path = skillPath(sharing.skillSource(target) ?? target, name);
    } else if (item.kind === "command") {
      path = commandPath(sharing.commandSource(target) ?? target, name);
    } else if (item.kind === "agent" && target.base.supportsAgents) {
      path = agentPath(target, name);
    } else if (item.kind === "agent") {
      // Belay links an agent that the target cannot write to the copy of
      // another tool, or else documents it in the reference tree.
      const peer = sharing.agentSource(target);
      if (peer === null) continue;
      path = agentPath(peer, name);
    } else {
      continue;
    }
    natives.set(item.anchor, path);
  }

  const documented = new Set<number>([...emission.documents, ...emission.shapeInstructions]);
  for (const anchor of closure.referenced) documented.add(anchor);
  const compiledShape = shapeRoot(target);
  const byPath = new Map<string, number[]>();
  for (const anchor of [...documented].sort((a, b) => a - b)) {
    if (natives.has(anchor)) continue;
    const file = source.file(anchor);
    if (file.startsWith("@")) continue;
    const path = documentPath(
      file,
      project.sourceRoot,
      project.root,
      project.shapeRoot,
      target.referenceRoot,
      compiledShape,
    );
    const group = byPath.get(path);
    if (group === undefined) byPath.set(path, [anchor]);
    else group.push(anchor);
  }
  // Anchors keep the order of their declarations in the file of their module.
  for (const anchors of byPath.values()) anchors.sort((a, b) => source.anchor(a).item - source.anchor(b).item);
  const files = [...byPath.entries()].sort(([a], [b]) => comparePaths(a, b));

  const spec: LinkLayout = {
    documents: files.map(([path, anchors]) => ({
      path,
      anchors: anchors.map((id) => {
        const omitNull = omitted(world, id);
        return omitNull.length > 0 ? { id, omitNull } : { id };
      }),
    })),
    files: [...natives.entries()].map(([id, path]) => ({ id, path })),
  };
  const space = source.compilation.linkSpace(spec);
  return { context: { world, target, space }, files, compiledShape, spec };
}

/** Where one target puts each anchor, as a build lays it out, for a command
 * that cites the compiled output instead of the source. A target that the
 * project does not build gives an error. */
export function targetLayout(world: World, id: string): { layout: LinkLayout } | { error: string } {
  const targets = plannedTargets(world.options);
  const found = targets.find((target) => target.id === id);
  if (found === undefined) {
    const built = world.options.targets.map((target) => `\`${target.id}\``);
    return {
      error:
        built.length === 0
          ? `\`${id}\` is not a target this project builds; it builds none`
          : `\`${id}\` is not a target this project builds; it builds ${built.join(", ")}`,
    };
  }
  const emission = collectEmission(world, new Draft());
  const closure = close(world.source, roots(emission));
  const sharing = new Sharing(shared(world.options), targets);
  return { layout: layout(world, found, sharing, emission, closure).spec };
}

function buildTarget(
  world: World,
  target: Target,
  sharing: Sharing,
  emission: Emission,
  closure: Closure,
  draft: Draft,
): void {
  const { source } = world;
  const first = draft.files.length;
  const { context, files, compiledShape } = layout(world, target, sharing, emission, closure);
  const { space } = context;

  if (sharing.ownsReferences(target)) {
    for (const [path, anchors] of files) {
      const parts = anchors.map((id) => space.document(path, id, omitted(world, id)));
      draft.push({
        path,
        contents: `${parts.map(trimEnd).join("\n\n")}\n`,
        kind: startsWith(path, compiledShape) ? "shape reference" : "reference",
        target: target.id,
        origin: anchors.map((id) => source.name(id)).join(", "),
        copied: null,
        anchors,
      });
    }
  }

  // Constructs.
  const instructions = new Map<string, InstructionSection[]>();
  for (const item of emission.constructs) {
    switch (item.kind) {
      case "skill": {
        // Belay does not write again a skill that the tool already
        // discovers in the directory of another target.
        const peer = sharing.skillSource(target);
        if (peer !== null) reportUnused(source, item, target, peer, draft);
        else renderSkill(context, item, draft);
        break;
      }
      case "command":
        if (sharing.commandSource(target) === null) renderCommand(context, item, draft);
        break;
      case "agent":
        if (!target.base.supportsAgents) {
          draft.diagnostics.push(
            warning(
              "unsupported-construct",
              `${target.anchorName} has no project-local agent definition, so the agent \`${item.name}\` is left out of its output`,
              source.anchorSite(item.anchor),
              {
                help: `${target.id} still gets everything else; make it a skill if it should load there, or build the agent for an adapter that supports agents`,
              },
            ),
          );
        } else {
          renderAgent(context, item, draft);
        }
        break;
      case "instruction": {
        const scope = emission.placements.get(item.anchor);
        if (scope === undefined) break;
        const path = normalize(join(scope, target.instructionFile));
        const section = instructionSection(context, item, path);
        const group = instructions.get(path);
        if (group === undefined) instructions.set(path, [section]);
        else group.push(section);
        break;
      }
    }
  }

  // Belay combines the guidance for one scope into one file for each target,
  // in a stable order: source path, then declaration order. Thus the same
  // input gives the same bytes.
  for (const [path, sections] of [...instructions.entries()].sort(([a], [b]) => comparePaths(a, b))) {
    sections.sort((a, b) => comparePaths(a.source, b.source) || a.item - b.item);
    draft.push({
      path,
      contents: `${sections.map((section) => trimEnd(section.text)).join("\n\n")}\n`,
      kind: "instruction",
      target: target.id,
      origin: sections.map((section) => section.name).join(", "),
      copied: null,
      anchors: sections.map((section) => section.anchor),
    });
  }

  // Belay reported a reference without a location when it made the closure.
  // Anything else is a gap in the plan, and Belay reports it here.
  const report = space.report();
  for (const { from, link } of report.links) draft.addLink(from, link);
  const missing = new Set<number>();
  for (const miss of report.misses) {
    if (closure.unrepresentable.has(miss.id) || missing.has(miss.id)) continue;
    missing.add(miss.id);
    draft.diagnostics.push(
      error(
        "missing-reference-target",
        `\`@{${display(source, { anchor: miss.id, path: miss.path })}}\` has no planned location for ${target.id}`,
        source.anchorSite(miss.id),
      ),
    );
  }

  // Belay resolves the location markers of each file that this target
  // planned. It does this here, not at each construction, so that a skill and
  // a reference document agree about where `.claude` is.
  const markers = locationMarkers(world, target);
  for (const file of draft.files.slice(first)) {
    if (file.copied !== null || !file.contents.includes("")) continue;
    for (const [marker, destination] of markers) {
      if (file.contents.includes(marker)) {
        file.contents = file.contents.replaceAll(marker, source.ctx.relativePath(file.path, destination));
      }
    }
  }

  instructionFiles(source, target, draft.files.slice(first), draft.diagnostics);
}

/** Each location marker, with the directory that it names, relative to the
 * project root. `shapeRoot` and `codeRoot` fall back to the project root, as
 * the specification says an unconfigured root does. */
function locationMarkers(world: World, target: Target): [string, string][] {
  const project = world.source.project;
  const marker = (name: string) => world.markers[name];
  return [
    [marker("BELAY_COMPILED_SHAPE"), shapeRoot(target)],
    [marker("BELAY_AGENT_ROOT"), target.root],
    [marker("BELAY_PROJECT_ROOT"), ""],
    [marker("BELAY_SHAPE_ROOT"), project.shapeRoot === null ? "" : underRoot(project.shapeRoot, project.root)],
    [marker("BELAY_CODE_ROOT"), underRoot(project.codeRoot, project.root)],
  ];
}

