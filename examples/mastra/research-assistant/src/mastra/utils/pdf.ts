import pdfParse from 'pdf-parse';
import { getAgentFS } from './agentfs.js';

async function fetchHtmlWithFallback(url: string): Promise<string> {
  const res = await fetch(url, {
    headers: {
      'User-Agent':
        'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36',
      Accept: 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8',
      'Accept-Language': 'en-US,en;q=0.9',
      Referer: url,
    },
  });
  const text = await res.text();
  if (!res.ok || /Access Denied|Robot Check|unusual traffic/i.test(text)) {
    try {
      const u = new URL(url);
      const proxy = `https://r.jina.ai/http://${u.host}${u.pathname}${u.search || ''}`;
      const proxRes = await fetch(proxy, { headers: { Accept: 'text/plain' } });
      if (proxRes.ok) {
        return await proxRes.text();
      }
    } catch {
      // ignore
    }
  }
  return text;
}

export async function fetchPdfText(pdfUrl: string, maxBytes = 20_000_000): Promise<string> {
  const res = await fetch(pdfUrl, {
    headers: {
      'User-Agent':
        'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36',
      Accept: 'application/pdf',
    },
  });
  if (!res.ok) {
    throw new Error(`PDF fetch failed: ${res.status}`);
  }
  const buf = Buffer.from(await res.arrayBuffer());
  if (buf.byteLength > maxBytes) {
    throw new Error(`PDF too large: ${buf.byteLength} bytes`);
  }
  const parsed = await pdfParse(buf);
  return parsed.text || '';
}

export async function fetchPdfTextCached(cacheKey: string, pdfUrl: string, maxBytes = 20_000_000): Promise<string> {
  const fs = (await getAgentFS()).fs;
  const safeKey = cacheKey.replace(/[^a-z0-9-_]/gi, '_').toLowerCase().slice(0, 200);
  const pdfPath = `/papers/${safeKey}.pdf`;
  const txtPath = `/papers/${safeKey}.txt`;
  // Try text cache
  try {
    const text = await fs.readFile(txtPath);
    if (text && text.length > 0) return text;
  } catch {
    // not cached
  }
  // Download PDF
  const res = await fetch(pdfUrl, {
    headers: {
      'User-Agent':
        'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36',
      Accept: 'application/pdf',
      Referer: pdfUrl,
    },
  });
  if (!res.ok) {
    throw new Error(`PDF fetch failed: ${res.status}`);
  }
  const buf = Buffer.from(await res.arrayBuffer());
  if (buf.byteLength > maxBytes) {
    throw new Error(`PDF too large: ${buf.byteLength} bytes`);
  }
  // Cache PDF
  try {
    await fs.writeFile(pdfPath, buf);
  } catch {
    // ignore cache write error
  }
  const parsed = await pdfParse(buf);
  const text = parsed.text || '';
  // Cache text
  try {
    await fs.writeFile(txtPath, text);
  } catch {
    // ignore
  }
  return text;
}

export function clipText(text: string, maxChars = 8000): string {
  if (text.length <= maxChars) return text;
  return text.slice(0, maxChars);
}

export async function resolvePdfUrl(landingUrl: string): Promise<string | undefined> {
  try {
    const u = new URL(landingUrl);
    // Direct PDF link
    if (/\.(pdf)(\?|#|$)/i.test(u.pathname)) {
      return landingUrl;
    }
    // arXiv: handle abs -> pdf and pdf without extension
    if (u.host.includes('arxiv.org')) {
      const absMatch = u.pathname.match(/^\/abs\/(.+)$/i);
      if (absMatch && absMatch[1]) {
        return `https://arxiv.org/pdf/${absMatch[1]}.pdf`;
      }
      const pdfMatch = u.pathname.match(/^\/pdf\/(.+)$/i);
      if (pdfMatch && pdfMatch[1] && !pdfMatch[1].endsWith('.pdf')) {
        return `https://arxiv.org/pdf/${pdfMatch[1]}.pdf`;
      }
    }
    // ACM DL DOI landing page
    if (u.host.includes('dl.acm.org') && /\/doi\//.test(u.pathname)) {
      const html = await fetchHtmlWithFallback(landingUrl);
      // 1) meta citation_pdf_url
      const meta = html.match(/<meta\s+name=["']citation_pdf_url["']\s+content=["']([^"']+)["']/i);
      if (meta?.[1]) {
        const abs = new URL(meta[1], landingUrl).toString();
        return abs;
      }
      // 2) explicit /doi/pdf or /doi/pdfdirect links
      const m = html.match(/href=["'](\/doi\/pdf(?:direct)?\/[^"']+)["']/i);
      if (m?.[1]) {
        const abs = new URL(m[1], landingUrl).toString();
        return abs;
      }
      // 3) try OpenGraph pdf link
      const og = html.match(/<meta\s+property=["']og:url["']\s+content=["']([^"']+)["']/i);
      if (og?.[1] && /\/doi\/pdf/.test(og[1])) {
        return new URL(og[1], landingUrl).toString();
      }
    }
    // DOI resolver: if arXiv DOI, map to arXiv pdf
    if (u.host.includes('doi.org')) {
      const arxivId = landingUrl.match(/10\.48550\/arXiv\.(\d+\.\d+)(v\d+)?/i);
      if (arxivId?.[1]) {
        return `https://arxiv.org/pdf/${arxivId[1]}.pdf`;
      }
    }
  } catch {
    // ignore
  }
  return undefined;
}


