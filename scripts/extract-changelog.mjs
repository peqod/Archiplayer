import { readFile } from "node:fs/promises";

// Prints the CHANGELOG.md body for one release, so CI can hand it to
// tauri-action as the release notes. Throws when the section is missing, which
// is the point: release.ps1 runs this before the tag push, so a forgotten
// changelog entry stops the release while stopping is still free.

const tag = process.argv[2];
if (!tag) {
  throw new Error("Usage: node scripts/extract-changelog.mjs vX.Y.Z");
}

const version = tag.replace(/^v/, "");
const changelog = await readFile(new URL("../CHANGELOG.md", import.meta.url), "utf8");

// Heading shapes accepted: "## [0.4.0] - 2026-07-27" and "## 0.4.0".
// Escape the version because dots would otherwise match any character.
const escaped = version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const heading = new RegExp(`^##\\s+\\[?${escaped}\\]?(\\s|$).*$`, "m");
const start = changelog.match(heading);
if (!start) {
  throw new Error(`No CHANGELOG.md section for ${version}. Add one before releasing ${tag}.`);
}

const after = changelog.slice(start.index + start[0].length);
// Stop at the next release heading. "### Added" belongs to this section and does
// not match, because it has no whitespace after the second hash.
const next = after.match(/^##\s/m);
const body = (next ? after.slice(0, next.index) : after).trim();

if (!body) {
  throw new Error(`The CHANGELOG.md section for ${version} is empty.`);
}

console.log(body);
