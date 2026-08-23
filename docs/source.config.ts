import { defineConfig, defineDocs } from 'fumadocs-mdx/config';
import { remarkAlert } from 'remark-github-blockquote-alert';
import { remarkMermaid } from '@theguild/remark-mermaid';

export const docs = defineDocs({});

export default defineConfig({
  mdxOptions: {
    remarkImageOptions: {
      // Keep /images/... URLs as-is and serve them from public/,
      // matching the previous Mintlify setup
      useImport: false,
    },
    // GitHub-style alert blockquotes (> [!WARNING]) render as styled alerts;
    // fenced mermaid blocks render as diagrams
    remarkPlugins: [remarkAlert, remarkMermaid],
  },
});
