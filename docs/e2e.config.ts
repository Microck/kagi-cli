import type { E2EConfig } from "e2e";
import { web } from "@e2e-dev/web";
export default {
 tests:"tests/**/*.e2e.ts",workers:1,retries:0,timeout:30_000,
 targets:[{name:"chromium",engine:web(),app:{url:"http://127.0.0.1:4182",command:{"executable": "node", "args": ["node_modules/next/dist/bin/next", "start", "--hostname", "127.0.0.1", "--port", "4182"]}}}],
} satisfies E2EConfig;
