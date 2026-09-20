import fs from "node:fs";
import crypto from "node:crypto";
import assert from "node:assert/strict";
const pkg = JSON.parse(fs.readFileSync("package.json"));
const root = "release-assets";
const files = fs
  .readdirSync(root)
  .filter((n) => n.endsWith(".dmg"))
  .sort();
assert.equal(files.length, 2, "Both CPU architectures must be present");
assert.equal(files.filter((f) => f.endsWith("_aarch64.dmg")).length, 1);
assert.equal(files.filter((f) => f.endsWith("_x64.dmg")).length, 1);
const artifacts = files.map((file) => ({
  file,
  sha256: crypto
    .createHash("sha256")
    .update(fs.readFileSync(`${root}/${file}`))
    .digest("hex"),
}));
fs.writeFileSync(
  `${root}/SHA256SUMS`,
  artifacts.map((a) => `${a.sha256}  ${a.file}`).join("\n") + "\n",
);
fs.writeFileSync(
  `${root}/release-manifest.json`,
  JSON.stringify(
    {
      version: pkg.version,
      commit: process.env.GITHUB_SHA,
      minimumMacOS: "13.0",
      schema: 1,
      parser: "codex-claude-v2",
      metric: "resona-v1",
      artifacts,
    },
    null,
    2,
  ) + "\n",
);
const changelog = fs.readFileSync("CHANGELOG.md", "utf8");
const section = changelog.split(`## [${pkg.version}]`)[1]?.split("\n## [")[0]?.trim();
assert.ok(section, "Changelog section is required");
fs.writeFileSync(`${root}/RELEASE_NOTES.md`, section + "\n");
