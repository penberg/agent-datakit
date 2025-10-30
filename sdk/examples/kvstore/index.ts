import { AgentOS } from "agentos-sdk";

async function main() {
  // Initialize AgentOS (in-memory for this example)
  const agent = new AgentOS(":memory:");

  console.log("=== KvStore Example ===\n");

  // Example 1: Store and retrieve simple values
  console.log("1. Storing simple values:");
  await agent.kv.set("username", "alice");
  await agent.kv.set("age", 30);
  await agent.kv.set("active", true);

  const username = await agent.kv.get("username");
  const age = await agent.kv.get("age");
  const active = await agent.kv.get("active");

  console.log(`  Username: ${username}`);
  console.log(`  Age: ${age}`);
  console.log(`  Active: ${active}\n`);

  // Example 2: Store and retrieve objects
  console.log("2. Storing complex objects:");
  const user = {
    id: 1,
    name: "Alice Johnson",
    email: "alice@example.com",
    preferences: {
      theme: "dark",
      notifications: true
    }
  };

  await agent.kv.set("user:1", user);
  const retrievedUser = await agent.kv.get("user:1");
  console.log("  Stored user:", JSON.stringify(retrievedUser, null, 2), "\n");

  // Example 3: Store and retrieve arrays
  console.log("3. Storing arrays:");
  const tags = ["typescript", "database", "ai", "agent"];
  await agent.kv.set("tags", tags);
  const retrievedTags = await agent.kv.get("tags");
  console.log(`  Tags: ${retrievedTags.join(", ")}\n`);

  // Example 4: Update existing values
  console.log("4. Updating existing values:");
  console.log(`  Age before update: ${await agent.kv.get("age")}`);
  await agent.kv.set("age", 31);
  console.log(`  Age after update: ${await agent.kv.get("age")}\n`);

  // Example 5: Delete values
  console.log("5. Deleting values:");
  console.log(`  Username before delete: ${await agent.kv.get("username")}`);
  await agent.kv.delete("username");
  console.log(`  Username after delete: ${await agent.kv.get("username")}\n`);

  // Example 6: Handle non-existent keys
  console.log("6. Retrieving non-existent keys:");
  const nonExistent = await agent.kv.get("does-not-exist");
  console.log(`  Result: ${nonExistent}\n`);

  // Example 7: Use cases for AI agents
  console.log("7. AI Agent use cases:");

  // Session state
  await agent.kv.set("session:current", {
    conversationId: "conv-123",
    userId: "user-456",
    startTime: Date.now()
  });

  // Agent memory
  await agent.kv.set("memory:user-preferences", {
    language: "en",
    responseStyle: "concise",
    expertise: "intermediate"
  });

  // Task queue
  await agent.kv.set("tasks:pending", [
    { id: 1, task: "Process document", priority: "high" },
    { id: 2, task: "Send notification", priority: "low" }
  ]);

  console.log("  Session:", JSON.stringify(await agent.kv.get("session:current"), null, 2));
  console.log("  Memory:", JSON.stringify(await agent.kv.get("memory:user-preferences"), null, 2));
  console.log("  Tasks:", JSON.stringify(await agent.kv.get("tasks:pending"), null, 2));

  console.log("\n=== Example Complete ===");
}

main().catch(console.error);
