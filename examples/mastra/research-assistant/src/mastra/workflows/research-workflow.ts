import { createStep, createWorkflow } from '@mastra/core/workflows';
import { z } from 'zod';
import { searchSemanticScholar, paperSchema, Paper } from '../tools/semantic-tool';
import { rankByLexicalScore } from '../scorers/lexical-scorer';

const querySchema = z.object({
  question: z.string().min(3).describe('Research question about databases'),
});

const paperWithWhySchema = paperSchema.extend({
  relevance: z.number(),
  why: z.string(),
});

const searchStep = createStep({
  id: 'search-semanticscholar',
  description: 'Search Semantic Scholar for VLDB/SIGMOD papers relevant to the question',
  inputSchema: querySchema,
  outputSchema: z.object({
    candidates: z.array(paperSchema),
  }),
  execute: async ({ inputData }) => {
    if (!inputData) throw new Error('Input data not found');
    const candidates: Paper[] = await searchSemanticScholar(inputData.question);
    return { candidates };
  },
});

const rankStep = createStep({
  id: 'rank-and-select',
  description: 'Rank candidates lexically and select top-5',
  inputSchema: z.object({
    question: z.string(),
    candidates: z.array(paperSchema),
  }),
  outputSchema: z.object({
    topPapers: z.array(paperWithWhySchema),
  }),
  execute: async ({ inputData }) => {
    if (!inputData) throw new Error('Input data not found');
    const ranked = rankByLexicalScore(inputData.question, inputData.candidates, 5);
    const topPapers = ranked.map(({ paper, score }) => {
      const why = buildWhy(inputData.question, paper);
      return { ...paper, relevance: Number(score.toFixed(3)), why };
    });
    return { topPapers };
  },
});

function buildWhy(question: string, paper: Paper): string {
  const qTokens = question
    .toLowerCase()
    .replace(/[^a-z0-9\s]/g, ' ')
    .split(/\s+/)
    .filter(Boolean);
  const matches = qTokens.filter((t) => paper.title.toLowerCase().includes(t));
  if (matches.length > 0) {
    return `Title matches keywords: ${Array.from(new Set(matches)).slice(0, 5).join(', ')}`;
  }
  return 'Relevant to query within VLDB/SIGMOD; selected via lexical similarity.';
}

const researchWorkflow = createWorkflow({
  id: 'research-workflow',
  inputSchema: querySchema,
  outputSchema: z.object({
    topPapers: z.array(paperWithWhySchema),
  }),
})
  .then(searchStep)
  .then(rankStep);

researchWorkflow.commit();

export { researchWorkflow };


