// This module makes the diagnostics of the plan.

import type { PlannedDiagnostic } from "@piton/plugin";
import type { Site } from "./source";

export type Diagnostic = PlannedDiagnostic;

/** An error at a place in a source file. */
export function error(code: string, message: string, site: Site, extra: { help?: string } = {}): Diagnostic {
  return withHelp({ severity: "error", code, message, location: site }, extra.help);
}

/** A warning at a place in a source file. */
export function warning(code: string, message: string, site: Site, extra: { help?: string } = {}): Diagnostic {
  return withHelp({ severity: "warning", code, message, location: site }, extra.help);
}

function withHelp(diagnostic: Diagnostic, help: string | undefined): Diagnostic {
  return help === undefined ? diagnostic : { ...diagnostic, help };
}
