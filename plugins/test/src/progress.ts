// This module shows the progress of `piton test` while the tools operate.
//
// A test can take minutes.
// If a run shows nothing until each test finishes, the run looks stopped.
// On a terminal, a status block shows below the results.
// The block shows each test that runs, its duration, and the last action of its tool.
// The block changes when this data changes.
// When a test finishes, its result shows above the block.
// In a log, the block causes only noise.
// Thus, in a log, a line shows when each test starts.

import type { Context } from "@piton/plugin";
import { clock, length, truncate } from "./output";

/** The frames of the spinner at the top of the block. */
const SPINNER = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

interface Running {
  name: string;
  started: number;
  activity: string;
}

export class Progress {
  /** If true, the block shows. The block needs a terminal. */
  private readonly live: boolean;
  private finished = 0;
  private readonly running = new Map<number, Running>();
  /** The number of lines of the block on the screen. The code uses this
   * number to erase the block. */
  private drawn = 0;
  private frame = 0;

  constructor(
    private readonly ctx: Context,
    private readonly total: number,
  ) {
    this.live = ctx.terminal.stderr;
  }

  /** The tool of a test started. */
  start(index: number, name: string): void {
    if (!this.live) this.ctx.stderr(`running ${name}\n`);
    this.running.set(index, { name, started: Date.now(), activity: "starting" });
    this.draw();
  }

  /** Records what the tool of a test does now. */
  activity(index: number, activity: string): void {
    const running = this.running.get(index);
    if (running !== undefined) running.activity = activity;
  }

  /** A test finished. Removes the block, so that the result of the test can
   * show in its place. Gives the duration of the test. */
  finish(index: number): number {
    this.clear();
    this.finished += 1;
    const running = this.running.get(index);
    this.running.delete(index);
    return running === undefined ? 0 : Date.now() - running.started;
  }

  /** Moves the spinner to the next frame and draws the block again. */
  tick(): void {
    this.frame += 1;
    this.draw();
  }

  private paint(style: "name" | "dim" | "heading", text: string): string {
    return this.ctx.terminal.color ? this.ctx.paint(style, text) : text;
  }

  /** Erases the block from the screen. */
  clear(): void {
    if (this.drawn === 0) return;
    // Move to the start of the first line of the block.
    // Then clear to the end of the screen.
    this.ctx.stderrRaw(`\x1b[${this.drawn}F\x1b[J`);
    this.drawn = 0;
  }

  /** Draws the block in the place of the block on the screen. */
  draw(): void {
    if (!this.live || this.running.size === 0) {
      this.clear();
      return;
    }
    const width = Math.max(this.ctx.terminal.width ?? 100, 20);
    const nameWidth = Math.max(...[...this.running.values()].map((running) => length(running.name)));
    const lines = [
      `${this.paint("name", SPINNER[this.frame % SPINNER.length])} ${this.paint(
        "dim",
        `${this.finished} of ${this.total} done, ${this.running.size} running`,
      )}`,
    ];
    for (const index of [...this.running.keys()].sort((a, b) => a - b)) {
      const running = this.running.get(index)!;
      const elapsed = clock(Date.now() - running.started);
      // Two spaces, the name with spaces after it, two spaces, the clock, and two spaces.
      const used = 2 + nameWidth + 2 + elapsed.length + 2;
      const activity = truncate(running.activity, width - used - 1);
      const name = truncate(running.name, width - 4).padEnd(nameWidth);
      lines.push(`  ${this.paint("heading", name)}  ${this.paint("dim", elapsed)}  ${this.paint("dim", activity)}`);
    }
    const erase = this.drawn > 0 ? `\x1b[${this.drawn}F\x1b[J` : "";
    // Write the text without changes. Thus, the cursor codes go to the terminal.
    // The text has colour only when the terminal shows colour.
    this.ctx.stderrRaw(erase + lines.map((line) => `${line}\n`).join(""));
    this.drawn = lines.length;
  }
}
