import { createTool } from '@mastra/core/tools';
import { z } from 'zod';
import { fetchPdfTextCached, resolvePdfUrl } from '../utils/pdf.js';
import { getAgentFS } from '../utils/agentfs.js';

export const pdfFetchTool = createTool({
  id: 'fetch-pdf-text',
  description:
    'Fetches and parses full text content from a research paper PDF URL. Use this when you need the complete paper content for detailed analysis beyond the abstract.',
  inputSchema: z.object({
    pdfUrl: z.string().url().describe('Direct URL to the PDF file'),
    paperTitle: z.string().describe('Title of the paper (used for caching)'),
    year: z.number().describe('Publication year (used for caching)'),
  }),
  outputSchema: z.object({
    text: z.string().describe('Full text content extracted from the PDF'),
    cached: z.boolean().describe('Whether the content was retrieved from cache'),
  }),
  execute: async ({ context }) => {
    const { pdfUrl, paperTitle, year } = context;

    // Generate cache key from title and year
    const cacheKey = `${paperTitle}-${year}`
      .toLowerCase()
      .replace(/[^a-z0-9-_]+/g, '-')
      .replace(/-+/g, '-')
      .replace(/^-|-$/g, '')
      .slice(0, 150);

    const agentfs = await getAgentFS();

    // Check if already cached
    const txtPath = `/papers/${cacheKey}.txt`;
    try {
      const cached = await agentfs.fs.readFile(txtPath);
      if (cached && cached.length > 300) {
        return { text: cached, cached: true };
      }
    } catch {
      // Not cached, proceed to fetch
    }

    // Fetch and parse PDF
    const text = await fetchPdfTextCached(cacheKey, pdfUrl);

    if (!text || text.length < 300) {
      throw new Error('PDF text extraction failed or returned insufficient content');
    }

    return { text, cached: false };
  },
});

export const resolvePdfUrlTool = createTool({
  id: 'resolve-pdf-url',
  description:
    'Attempts to resolve a landing page URL (like a DOI or ACM DL page) to a direct PDF URL. Use this when you have a paper URL but need the PDF link.',
  inputSchema: z.object({
    url: z.string().url().describe('Landing page URL for the paper'),
  }),
  outputSchema: z.object({
    pdfUrl: z.string().url().optional().describe('Resolved direct PDF URL, if found'),
    success: z.boolean().describe('Whether PDF URL resolution succeeded'),
  }),
  execute: async ({ context }) => {
    const { url } = context;
    const pdfUrl = await resolvePdfUrl(url);

    return {
      pdfUrl: pdfUrl || undefined,
      success: !!pdfUrl,
    };
  },
});
