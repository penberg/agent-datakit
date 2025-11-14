#!/usr/bin/env node
import 'dotenv/config';
import { runResearchAgent } from './agent.js';

async function main() {
  const args = process.argv.slice(2);
  const question = args.join(' ').trim();

  if (!question) {
    console.error('Usage: npm run ask -- "<your research question>"');
    process.exit(1);
  }

  try {
    console.log('Researching your question...\n');

    const response = await runResearchAgent(question);

    console.log(response);
  } catch (err: any) {
    console.error('Error:', err?.message || err);
    process.exit(1);
  }
}

main();
