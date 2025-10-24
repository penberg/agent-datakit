import { AgentOS } from '../../src';

async function main() {
  // Create an agent with an in-memory database
  const agent = new AgentOS(':memory:');
  await agent.ready();

  console.log('=== Tool Call Tracking Example ===\n');

  // Example 1: Successful tool call
  console.log('1. Tracking a successful web search:');
  const startTime1 = Date.now() / 1000;

  // Simulate some work
  await new Promise(resolve => setTimeout(resolve, 100));

  const endTime1 = Date.now() / 1000;
  const searchId = await agent.tools.record(
    'web_search',
    startTime1,
    endTime1,
    { query: 'AI agents and LLMs', maxResults: 10 },
    {
      results: [
        { title: 'Understanding AI Agents', url: 'https://example.com/1' },
        { title: 'LLM Best Practices', url: 'https://example.com/2' },
      ],
      count: 2,
    }
  );
  console.log(`   Recorded tool call with ID: ${searchId}\n`);

  // Example 2: Failed tool call
  console.log('2. Tracking a failed API call:');
  const startTime2 = Date.now() / 1000;

  await new Promise(resolve => setTimeout(resolve, 50));

  const endTime2 = Date.now() / 1000;
  const apiId = await agent.tools.record(
    'api_call',
    startTime2,
    endTime2,
    { endpoint: '/users', method: 'GET' },
    undefined,
    'Connection timeout after 30s'
  );
  console.log(`   Recorded failed call with ID: ${apiId}\n`);

  // Example 3: Multiple tool calls
  console.log('3. Tracking multiple database queries:');
  for (let i = 0; i < 3; i++) {
    const start = Date.now() / 1000;
    await new Promise(resolve => setTimeout(resolve, 20));
    const end = Date.now() / 1000;

    await agent.tools.record(
      'database_query',
      start,
      end,
      { sql: `SELECT * FROM users WHERE id = ${i + 1}` },
      { rows: 1 }
    );
  }
  console.log('   Created 3 database query records\n');

  // Query tool calls by name
  console.log('4. Querying tool calls by name:');
  const searches = await agent.tools.getByName('web_search');
  console.log(`   Found ${searches.length} web search calls`);
  if (searches.length > 0) {
    const search = searches[0];
    console.log(`   - Duration: ${search.duration_ms}ms`);
    console.log(`   - Parameters:`, JSON.stringify(search.parameters));
    console.log(`   - Result:`, JSON.stringify(search.result));
  }
  console.log();

  // Get recent tool calls
  console.log('5. Getting recent tool calls:');
  const oneMinuteAgo = Math.floor(Date.now() / 1000) - 60;
  const recent = await agent.tools.getRecent(oneMinuteAgo);
  console.log(`   Found ${recent.length} calls in the last minute:`);
  recent.forEach(tc => {
    const status = tc.error ? 'failed' : 'success';
    console.log(`   - ${tc.name} (${status})`);
  });
  console.log();

  // Get performance statistics
  console.log('6. Performance statistics:');
  const stats = await agent.tools.getStats();
  console.log('   Tool Performance:');
  stats.forEach(stat => {
    console.log(`   - ${stat.name}:`);
    console.log(`     Total: ${stat.total_calls}, Success: ${stat.successful}, Failed: ${stat.failed}`);
    console.log(`     Avg Duration: ${stat.avg_duration_ms.toFixed(2)}ms`);
  });

  // Clean up
  await agent.close();
  console.log('\n✓ Example completed successfully!');
}

main().catch(console.error);
