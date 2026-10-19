import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { spawnSync } from "node:child_process";
const require = createRequire(import.meta.url);
const [major, minor, patch] = process.versions.node.split(".").map(Number);
const supported22 = major === 22 && (minor > 22 || (minor === 22 && patch >= 3));
const supported24 = major > 24 || (major === 24 && minor >= 8);
if (!supported22 && !supported24) {
  console.error("Browser E2E requires Node ^22.22.3 or >=24.8.0.");
  process.exit(1);
}
const cli = join(dirname(require.resolve("e2e")), "cli", "bin.js");
const result = spawnSync(process.execPath, [cli, "run", ...process.argv.slice(2)], {
  stdio: "inherit",
  env: { ...process.env, E2E_TELEMETRY_DISABLED: "1" },
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
