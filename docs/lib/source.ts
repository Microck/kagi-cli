import { docs } from '@/.source';
import { loader } from 'fumadocs-core/source';

// Docs are served from the site root so existing /guides/... and
// /commands/... links keep working unchanged.
export const source = loader({
  source: docs.toFumadocsSource(),
  baseUrl: '/',
});
