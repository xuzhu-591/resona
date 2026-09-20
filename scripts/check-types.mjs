import { execFileSync } from "node:child_process";
import fs from "node:fs";
import { format } from "prettier";
const generated = execFileSync(
  "cargo",
  ["run", "--quiet", "--locked", "-p", "resona-core", "--example", "export_types"],
  { encoding: "utf8" },
);
const formatted = await format(generated, { parser: "typescript", printWidth: 100 });
if (process.argv.includes("--write")) fs.writeFileSync("src/api/types.ts", formatted);
else if (fs.readFileSync("src/api/types.ts", "utf8") !== formatted)
  throw new Error("IPC types differ; run pnpm types:generate");
