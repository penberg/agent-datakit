#!/usr/bin/env node
import 'dotenv/config';
import { run } from '@openai/agents';
import { researchAgent } from './agent.js';
import { getAgentFS } from './utils/agentfs.js';

async function main() {
  const args = process.argv.slice(2);
  const question = args.join(' ').trim();

  if (!question) {
    console.error('Usage: npm run ask -- "<your research question>"');
    process.exit(1);
  }

  try {
    // Initialize agentfs
    await getAgentFS();

    // Run the agent with the question
    const result = await run(researchAgent, question);

    // Output the agent's response
    console.log(result.finalOutput);

  } catch (err: any) {
    console.error('Error:', err?.message || err);
    process.exit(1);
  }
}

main();
