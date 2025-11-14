import { tool } from '@anthropic-ai/claude-agent-sdk';
import { z } from 'zod';
import { fetchPdfTextCached, resolvePdfUrl } from '../utils/pdf.js';
import { getAgentFS } from '../utils/agentfs.js';

export const pdfFetchTool = tool(
  'fetch-pdf-text',
  'Fetches and parses full text content from a research paper PDF URL. Use this when you need the complete paper content for detailed analysis beyond the abstract.',
  {
    pdfUrl: z.string().url().describe('Direct URL to the PDF file'),
    paperTitle: z.string().describe('Title of the paper (used for caching)'),
    year: z.number().describe('Publication year (used for caching)'),
  },
  async ({ pdfUrl, paperTitle, year }) => {
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
        return {
          content: [{
            type: 'text' as const,
            text: JSON.stringify({ text: cached, cached: true }, null, 2),
          }],
        };
      }
    } catch {
      // Not cached, proceed to fetch
    }

    // Fetch and parse PDF
    const text = await fetchPdfTextCached(cacheKey, pdfUrl);

    if (!text || text.length < 300) {
      throw new Error('PDF text extraction failed or returned insufficient content');
    }

    return {
      content: [{
        type: 'text' as const,
        text: JSON.stringify({ text, cached: false }, null, 2),
      }],
    };
  }
);

export const resolvePdfUrlTool = tool(
  'resolve-pdf-url',
  'Attempts to resolve a landing page URL (like a DOI or ACM DL page) to a direct PDF URL. Use this when you have a paper URL but need the PDF link.',
  {
    url: z.string().url().describe('Landing page URL for the paper'),
  },
  async ({ url }) => {
    const pdfUrl = await resolvePdfUrl(url);

    return {
      content: [{
        type: 'text' as const,
        text: JSON.stringify({
          pdfUrl: pdfUrl || undefined,
          success: !!pdfUrl,
        }, null, 2),
      }],
    };
  }
);
