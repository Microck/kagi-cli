import { ImageZoom } from 'fumadocs-ui/components/image-zoom';
import defaultMdxComponents from 'fumadocs-ui/mdx';
import type { MDXComponents } from 'mdx/types';
import { type ComponentProps } from 'react';

export function getMDXComponents(components?: MDXComponents): MDXComponents {
  return {
    ...defaultMdxComponents,
    // fumadocs-mdx compiles local images into static imports; ImageZoom
    // understands both URLs and StaticImageData
    img: ({ src, ...props }: ComponentProps<'img'> & { src?: unknown }) => (
      <ImageZoom src={src as string} {...props} />
    ),
    ...components,
  };
}
