// This module plans the Belay build.
//
// The build reads the project, plans each target, and gives the host the
// plan. The host checks the plan, writes the files, records them in the
// manifest and removes the files that an earlier build wrote and this build
// does not write.

import type { Context, Plan, PlannedCopy, PlannedFile } from "@piton/plugin";
import { Constructs } from "./constructs";
import { loadOptions } from "./options";
import { stripPrefix } from "./paths";
import { plan } from "./plan";
import { Source } from "./source";

export function build(ctx: Context): Plan {
  const source = new Source(ctx);
  const options = loadOptions(source);
  const draft = plan({ source, constructs: new Constructs(source), options, markers: ctx.placeholders() });

  const files: PlannedFile[] = [];
  const copies: PlannedCopy[] = [];
  for (const file of draft.files) {
    const label = `${file.kind}, ${file.target}`;
    if (file.copied === null) {
      files.push({ path: file.path, contents: file.contents, origin: file.origin, label, group: file.target });
    } else {
      const from = stripPrefix(file.copied.source, source.project.root) ?? file.copied.source;
      copies.push({ from, to: file.path, origin: file.origin, label, group: file.target });
    }
  }
  const targets: Record<string, string> = {};
  for (const target of options.targets) targets[target.id] = target.documentationChecked;
  return { files, copies, diagnostics: draft.diagnostics, targets };
}
