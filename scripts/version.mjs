import fs from "node:fs";
import assert from "node:assert/strict";
const [mode, version] = process.argv.slice(2);
assert.match(version ?? "", /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/);
const pkg = JSON.parse(fs.readFileSync("package.json"));
const config = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json"));
if (mode === "--check") {
  assert.equal(pkg.version, version);
  assert.equal(config.version, version);
  assert.ok(fs.readFileSync("Cargo.toml", "utf8").includes(`version = "${version}"`));
  assert.ok(fs.readFileSync("CHANGELOG.md", "utf8").includes(`## [${version}]`));
} else if (mode === "--sync") {
  pkg.version = version;
  config.version = version;
  fs.writeFileSync("package.json", JSON.stringify(pkg, null, 2) + "\n");
  fs.writeFileSync("src-tauri/tauri.conf.json", JSON.stringify(config, null, 2) + "\n");
  fs.writeFileSync(
    "Cargo.toml",
    fs.readFileSync("Cargo.toml", "utf8").replace(/^version = ".*"$/m, `version = "${version}"`),
  );
  console.log("Version updated. Refresh Cargo.lock before committing. No tag or release created.");
} else throw Error("Use --check or --sync VERSION");
