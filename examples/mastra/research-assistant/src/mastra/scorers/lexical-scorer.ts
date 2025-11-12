export type RankablePaper = {
  paperId: string;
  title: string;
  authors: string[];
  venue: string;
  year: number;
  url: string;
  abstractText?: string;
};

function tokenize(text: string): string[] {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9\s]/g, ' ')
    .split(/\s+/)
    .filter(Boolean);
}

function unique<T>(arr: T[]): T[] {
  return Array.from(new Set(arr));
}

export function computeLexicalScore(query: string, paper: RankablePaper): number {
  const queryTokens = unique(tokenize(query));
  const hay = [paper.title, paper.abstractText || ''].join(' ');
  const docTokens = tokenize(hay);

  if (docTokens.length === 0 || queryTokens.length === 0) return 0;

  // Simple term overlap weighted by term frequency in the document
  const freq: Record<string, number> = {};
  for (const t of docTokens) {
    freq[t] = (freq[t] || 0) + 1;
  }
  let score = 0;
  for (const qt of queryTokens) {
    if (freq[qt]) score += 1 + Math.log(1 + freq[qt]); // mild TF weight
  }

  // Small recency boost
  const currentYear = new Date().getFullYear();
  let recency = 0;
  if (paper.year && paper.year >= 1970 && paper.year <= currentYear + 1) {
    const age = currentYear - paper.year;
    recency = Math.max(0, 1 - Math.min(10, age) / 10); // 0..1
  }
  score += 0.1 * recency;

  return score;
}

export function rankByLexicalScore(query: string, papers: RankablePaper[], k = 5) {
  const withScores = papers.map((p) => ({
    paper: p,
    score: computeLexicalScore(query, p),
  }));
  withScores.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    if (b.paper.year !== a.paper.year) return b.paper.year - a.paper.year; // recent first
    return a.paper.title.localeCompare(b.paper.title);
  });
  return withScores.slice(0, k);
}


