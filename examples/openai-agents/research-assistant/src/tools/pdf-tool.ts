import { tool } from '@openai/agents';
import { z } from 'zod';
import { fetchPdfTextCached, resolvePdfUrl } from '../utils/pdf.js';
import { getAgentFS } from '../utils/agentfs.js';

export const pdfFetchTool = tool({
  name: 'fetch_pdf_text',
  description:
    'Fetches and parses full text content from a research paper PDF URL. Use this when you need the complete paper content for detailed analysis beyond the abstract.',
  parameters: z.object({
    pdfUrl: z.string().describe('Direct URL to the PDF file'),
    paperTitle: z.string().describe('Title of the paper (used for caching)'),
    year: z.number().describe('Publication year (used for caching)'),
  }),
  execute: async ({ pdfUrl, paperTitle, year }) => {
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

export const resolvePdfUrlTool = tool({
  name: 'resolve_pdf_url',
  description:
    'Attempts to resolve a landing page URL (like a DOI or ACM DL page) to a direct PDF URL. Use this when you have a paper URL but need the PDF link.',
  parameters: z.object({
    url: z.string().describe('Landing page URL for the paper'),
  }),
  execute: async ({ url }) => {
    const pdfUrl = await resolvePdfUrl(url);

    return {
      pdfUrl: pdfUrl || undefined,
      success: !!pdfUrl,
    };
  },
});
