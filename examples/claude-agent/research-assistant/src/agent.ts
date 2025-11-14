import { query, createSdkMcpServer } from '@anthropic-ai/claude-agent-sdk';
import { semanticScholarTool } from './tools/semantic-tool.js';
import { pdfFetchTool, resolvePdfUrlTool } from './tools/pdf-tool.js';
import { getAgentFS } from './utils/agentfs.js';

const AGENT_INSTRUCTIONS = `
You are a database research expert who provides comprehensive, evidence-based analysis.

When a user asks a research question:

1. **Search for papers**: Use the 'search-semanticscholar-vldb-sigmod' tool with the user's query to find relevant papers from VLDB and SIGMOD venues.

2. **Get full paper content**: For the most relevant papers (typically 3-5 papers):
   - If a paper has a pdfUrl, use the 'fetch-pdf-text' tool to get the complete paper text
   - If a paper only has a regular URL, first try 'resolve-pdf-url' to find the PDF link, then fetch it
   - Read the FULL TEXT carefully to extract specific findings, numbers, and comparisons
   - If PDF fetching fails, work with the abstract, but note the limitation

3. **Analyze the content**: Review the full paper texts, focusing on:
   - Key findings and performance metrics (extract specific numbers!)
   - Comparisons and benchmarks between systems
   - Trade-offs and limitations discussed by authors
   - Theoretical and practical insights
   - Methodologies and experimental setups

4. **Synthesize a comprehensive answer** that:
   - Directly answers the research question with specific evidence from the papers
   - Includes concrete performance numbers, comparisons, and trade-offs
   - Cites sources using the format (FirstAuthor, Year) - e.g., (Marcus, 2020), (Liu, 2024)
   - Uses semicolons for multiple citations: (Author1, Year1; Author2, Year2)
   - Highlights areas of agreement or disagreement between papers
   - Organizes findings into clear sections with headers like "Key Findings", "Performance Metrics", "Trade-offs"
   - Mentions when papers show different results or conflicting evidence

5. **Include a references section** at the end with full citations in this format:
   - AuthorLastName et al. (Year). Paper Title — Venue Year
   - URL

CRITICAL RULES:
- DO NOT just list papers. ANSWER the question with evidence!
- DO NOT give superficial summaries. Include SPECIFIC DETAILS from the papers!
- ALWAYS fetch and read full PDFs for thorough analysis - abstracts alone are insufficient!
- Include ACTUAL NUMBERS and metrics from the papers, not vague descriptions!
- Be thorough, accurate, and evidence-based in your analysis!
`;

// Create an MCP server with our custom tools
const toolServer = createSdkMcpServer({
  name: 'research-tools',
  version: '1.0.0',
  tools: [semanticScholarTool, pdfFetchTool, resolvePdfUrlTool],
});

export async function runResearchAgent(question: string): Promise<string> {
  // Initialize AgentFS
  await getAgentFS();

  let fullResponse = '';

  // Run the agent with the Claude Agent SDK query function
  const result = query({
    prompt: question,
    options: {
      model: 'sonnet',
      systemPrompt: AGENT_INSTRUCTIONS,
      mcpServers: {
        'research-tools': toolServer,
      },
      maxTurns: 50,
      permissionMode: 'bypassPermissions',
    },
  });

  // Stream messages and collect the final response
  for await (const message of result) {
    if (message.type === 'assistant') {
      // Collect text from assistant messages
      for (const content of message.message.content) {
        if (content.type === 'text') {
          fullResponse += content.text;
        }
      }
    } else if (message.type === 'result') {
      // Final result message
      break;
    }
  }

  return fullResponse;
}
