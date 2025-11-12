import { createTool } from '@mastra/core/tools';
import { z } from 'zod';

export const paperSchema = z.object({
  paperId: z.string(),
  title: z.string(),
  authors: z.array(z.string()),
  venue: z.string(),
  year: z.number(),
  url: z.string().url(),
  abstractText: z.string().optional(),
  pdfUrl: z.string().url().optional(),
});
export type Paper = z.infer<typeof paperSchema>;

type S2Author = { name?: string };
type S2Venue = { displayName?: string } | null;
type S2ExternalIds = { DOI?: string | null };
type S2Paper = {
  paperId?: string;
  title?: string;
  year?: number;
  authors?: S2Author[];
  url?: string | null;
  abstract?: string | null;
  venue?: string | null;
  publicationVenue?: S2Venue;
  externalIds?: S2ExternalIds;
  isOpenAccess?: boolean;
  openAccessPdf?: { url?: string | null } | null;
};

type S2SearchResponse = {
  data?: S2Paper[];
  total?: number;
};

function isTargetVenue(venue?: string | null): boolean {
  if (!venue) return false;
  const v = venue.toLowerCase();
  return (
    v.includes('sigmod') ||
    v.includes('acm sigmod') ||
    v.includes('sigmod conference') ||
    v.includes('proceedings of the vldb endowment') ||
    v.includes('pvl db') ||
    v.includes('pvlb') ||
    v.includes('pvlbd') ||
    v.includes('vldb') // keep broad to catch PVLDB; risk: VLDB Journal may appear
  );
}

async function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

async function fetchWithBackoff(url: string, init: RequestInit, maxRetries = 5, initialDelayMs = 500): Promise<Response> {
  let attempt = 0;
  let delay = initialDelayMs;
  while (true) {
    const res = await fetch(url, init);
    if (res.status !== 429 && res.status < 500) {
      return res;
    }
    if (attempt >= maxRetries) {
      return res;
    }
    // Prefer Retry-After header if present
    const retryAfter = res.headers.get('Retry-After');
    let waitMs = delay;
    if (retryAfter) {
      const secs = Number(retryAfter);
      if (!Number.isNaN(secs) && secs > 0) waitMs = secs * 1000;
    }
    await sleep(waitMs);
    attempt += 1;
    delay = Math.min(delay * 2, 8000);
  }
}

export async function searchSemanticScholar(query: string): Promise<Paper[]> {
  const base = 'https://api.semanticscholar.org/graph/v1/paper/search';
  const params = new URLSearchParams({
    query,
    limit: '100',
    fields: 'title,venue,publicationVenue,year,authors,url,abstract,externalIds,isOpenAccess,openAccessPdf',
  });
  const url = `${base}?${params.toString()}`;
  const res = await fetchWithBackoff(
    url,
    {
      headers: {
        Accept: 'application/json',
        'User-Agent': 'research-assistant',
      },
    },
    5,
    500,
  );
  if (!res.ok) {
    throw new Error(`Semantic Scholar request failed with status ${res.status}`);
  }
  const data = (await res.json()) as S2SearchResponse;
  const arr = data.data || [];
  const mapped: Paper[] = arr
    .map((p) => {
      const title = (p.title || '').trim();
      const authors = (p.authors || []).map((a) => (a.name || '').trim()).filter(Boolean);
      const venue =
        (p.publicationVenue?.displayName || p.venue || '').toString().trim();
      const year = Number(p.year || 0);
      const doi = p.externalIds?.DOI || null;
      const rawUrl = (doi ? `https://doi.org/${doi}` : p.url || '').toString().trim();
      const url = rawUrl;
      const paperId = (doi || p.paperId || url || title).toString();
      const abstractText = (p.abstract || undefined)?.toString();
      let pdfUrl =
        (p.openAccessPdf?.url || undefined) ||
        (doi && doi.toLowerCase().startsWith('10.48550/arxiv.')
          ? `https://arxiv.org/pdf/${doi.toLowerCase().split('arxiv.')[1]}.pdf`
          : undefined);
      // Fallback: if the URL is an arXiv abstract, derive the PDF URL
      try {
        const u = new URL(url);
        if (u.host.includes('arxiv.org')) {
          const absMatch = u.pathname.match(/^\/abs\/(.+)$/i);
          if (absMatch && absMatch[1]) {
            pdfUrl = `https://arxiv.org/pdf/${absMatch[1]}.pdf`;
          } else {
            const pdfMatch = u.pathname.match(/^\/pdf\/(.+)$/i);
            if (pdfMatch && pdfMatch[1] && !pdfMatch[1].endsWith('.pdf')) {
              pdfUrl = `https://arxiv.org/pdf/${pdfMatch[1]}.pdf`;
            }
          }
        }
      } catch {
        // ignore URL parse errors
      }
      return {
        paperId,
        title,
        authors,
        venue,
        year,
        url,
        abstractText,
        pdfUrl,
      } as Paper;
    })
    .filter((p) => p.title && p.url);
  // Filter to target venues and dedupe by URL
  const filtered = mapped.filter((p) => isTargetVenue(p.venue));
  const seen = new Set<string>();
  const deduped = filtered.filter((p) => {
    const key = p.url;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  return deduped;
}

export const semanticScholarTool = createTool({
  id: 'search-semanticscholar-vldb-sigmod',
  description:
    'Search Semantic Scholar for papers relevant to a query, constrained to VLDB (PVLDB) and SIGMOD venues',
  inputSchema: z.object({
    query: z.string().describe('User research question or keywords'),
  }),
  outputSchema: z.object({
    papers: z.array(paperSchema),
  }),
  execute: async ({ context }) => {
    const papers = await searchSemanticScholar(context.query);
    return { papers };
  },
});


