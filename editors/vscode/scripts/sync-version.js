// Sets the extension's version to the Piton release it goes with.
//
//   node scripts/sync-version.js 0.2.3   ->  0.2.3
//
// With no version, it works one out the way the edge release does: the major
// and minor from the workspace `Cargo.toml`, the patch from the number of
// commits after the commit that last changed that version.

const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");

const root = path.resolve(__dirname, "../../..");
const manifest = path.resolve(__dirname, "../package.json");

function pitonVersionFromWorkspace() {
  const cargo = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
  const match = cargo.match(/^version = "(\d+)\.(\d+)\.\d+"/m);
  if (!match) throw new Error("no version in Cargo.toml");
  const git = (args) => execFileSync("git", args, { cwd: root }).toString().trim();
  const bump = git(["log", "-1", "--format=%H", "-G^version = ", "--", "Cargo.toml"]);
  if (!bump) throw new Error("no commit sets the version in Cargo.toml");
  const patch = git(["rev-list", "--count", `${bump}..HEAD`]);
  return `${match[1]}.${match[2]}.${patch}`;
}

const version = process.argv[2] || pitonVersionFromWorkspace();
if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`not a Piton version: ${version}`);

const text = fs.readFileSync(manifest, "utf8");
fs.writeFileSync(manifest, text.replace(/"version": "[^"]*"/, `"version": "${version}"`));
console.log(`Extension version: ${version}`);
