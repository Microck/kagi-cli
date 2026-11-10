import type { E2EConfig } from "e2e";
import { web } from "@e2e-dev/web";
export default {
 tests:"tests/**/*.e2e.ts",workers:1,retries:0,timeout:30_000,
 targets:[{name:"chromium",engine:web(),app:{url:"http://127.0.0.1:4182",command:{"executable": "node", "args": ["node_modules/sirv-cli/bin.js", "out", "--host", "127.0.0.1", "--port", "4182", "--quiet"],log:".e2e/results/static-preview.log"}}}],
} satisfies E2EConfig;
