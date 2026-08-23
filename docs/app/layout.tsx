import './global.css';
import { RootProvider } from 'fumadocs-ui/provider';
import type { Metadata } from 'next';
import type { ReactNode } from 'react';

export const metadata: Metadata = {
  title: {
    template: '%s | kagi CLI',
    default: 'kagi CLI documentation',
  },
  description:
    'Documentation for the kagi CLI, a command-line interface and MCP server for Kagi search, summarization, extraction, and automation.',
  metadataBase: new URL('https://kagi.micr.dev'),
  icons: '/images/favicon.png',
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" className="flex h-full flex-col antialiased" suppressHydrationWarning>
      <body className="flex min-h-full flex-col">
        <RootProvider>{children}</RootProvider>
      </body>
    </html>
  );
}
