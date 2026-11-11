// This module is the entry of the Belay plugin.
//
// Belay compiles the four constructs into what agentic tools read. Its build
// plans every output file. The host writes the plan. The plugin also adds
// `piton agent` and `piton belay slice`, and gives other plugins the agentic
// tools and their fluency prompt.

import { definePlugin } from "@piton/plugin";
import { agentCommand, agentsExport } from "./agents";
import { build } from "./build";
import { rootsExport } from "./source";
import { sliceCommand } from "./slice";

export default definePlugin({
  build,
  commands: { agent: agentCommand, slice: sliceCommand },
  exports: { agents: agentsExport, roots: rootsExport },
});
