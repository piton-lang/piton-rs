// The templates `piton init` offers, read from the plugin's own templates
// directory.
//
// Every directory in templates/ is a group of templates, like `belay`, and
// every directory in a group is one template, like `belay/application`: its
// files are what `piton init` writes. Groups and templates each have a
// `.template` file holding the one-line description the picker shows. Adding
// a template is adding a directory; nothing here lists them by name. A group
// holding a `.preferred` file is the one to recommend, and comes before the
// others.

import type { Context } from "@piton/plugin";

/** The file in each group and template that describes it, never written. */
const DESCRIPTION = ".template";

/** The file marking a group as the recommended one. */
const PREFERRED = ".preferred";

/** The directory in the plugin that holds the templates. */
const ROOT = "templates";

export interface Template {
  /** With its group, like `belay/application`. */
  name: string;
  description: string;
  /** What it writes, by path relative to the new project. */
  files: { path: string; contents: string }[];
}

export interface Group {
  name: string;
  description: string;
  /** Whether this is the group to recommend. It comes first. */
  preferred: boolean;
  templates: Template[];
}

/** Every group of templates, the preferred ones first, then by name. */
export function groups(ctx: Context): Group[] {
  const paths = ctx.resources(ROOT);
  const groupNames = unique(paths.filter((path) => path.includes("/")).map((path) => path.split("/")[0]));
  const found: Group[] = groupNames.map((group) => {
    const inGroup = paths
      .filter((path) => path.startsWith(`${group}/`))
      .map((path) => path.slice(group.length + 1));
    const templateNames = unique(inGroup.filter((path) => path.includes("/")).map((path) => path.split("/")[0]));
    if (templateNames.length === 0) throw new Error(`${ROOT}/${group} has no templates`);
    const templates = templateNames.map((template) => {
      const name = `${group}/${template}`;
      const files = inGroup
        .filter((path) => path.startsWith(`${template}/`))
        .map((path) => path.slice(template.length + 1))
        .filter((path) => path !== DESCRIPTION)
        .map((path) => ({ path, contents: ctx.resource(`${ROOT}/${name}/${path}`) }));
      if (files.length === 0) throw new Error(`${ROOT}/${name} has no files`);
      return { name, description: description(ctx, name, inGroup.includes(`${template}/${DESCRIPTION}`)), files };
    });
    return {
      name: group,
      description: description(ctx, group, inGroup.includes(DESCRIPTION)),
      preferred: inGroup.includes(PREFERRED),
      templates,
    };
  });
  // Stable, so the others stay in name order after the preferred ones.
  return [...found.filter((group) => group.preferred), ...found.filter((group) => !group.preferred)];
}

/** The first line of the `.template` file of `name`, which has to exist and
 * say something. */
function description(ctx: Context, name: string, exists: boolean): string {
  if (!exists) throw new Error(`${ROOT}/${name} has no ${DESCRIPTION} file`);
  const line = ctx.resource(`${ROOT}/${name}/${DESCRIPTION}`).split("\n")[0].trim();
  if (line.length === 0) throw new Error(`${ROOT}/${name}/${DESCRIPTION} is empty`);
  return line;
}

function unique(items: string[]): string[] {
  return [...new Set(items)].sort();
}
