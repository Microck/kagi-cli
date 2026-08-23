import { source } from '@/lib/source';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { DocsBody, DocsPage } from 'fumadocs-ui/page';
import { getMDXComponents } from '@/mdx-components';
import { notFound } from 'next/navigation';
import type { Metadata } from 'next';
import { Logo } from '@/components/logo';

export default async function Page(props: {
  params: Promise<{ slug?: string[] }>;
}) {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  const MDX = page.data.body;

  return (
    <DocsLayout
      tree={source.pageTree}
      nav={{ title: <Logo /> }}
      links={[
        { text: 'GitHub', url: 'https://github.com/Microck/kagi-cli' },
        { text: 'npm', url: 'https://www.npmjs.com/package/kagi-cli' },
        { text: 'Kagi', url: 'https://kagi.com' },
      ]}
    >
      <DocsPage toc={page.data.toc}>
        <DocsBody>
          <MDX />
        </DocsBody>
      </DocsPage>
    </DocsLayout>
  );
}

export function generateStaticParams() {
  return source.generateParams();
}

export async function generateMetadata(props: {
  params: Promise<{ slug?: string[] }>;
}) {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  return {
    title: page.data.title,
    description: page.data.description,
  };
}
