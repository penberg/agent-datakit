import { AgentFS, tursoSync } from "agentfs-sdk";

/**
 * Simple Turso Cloud Sync Example
 *
 * This example demonstrates the minimal setup for syncing an AgentFS instance
 * with Turso Cloud using environment variables.
 *
 * Prerequisites:
 * - Set TURSO_API_TOKEN environment variable with your Turso API token
 * - Optionally set TURSO_API_ORG (defaults to your primary org)
 *
 * The sync provider will automatically:
 * 1. Create a database named 'simple-agent' if it doesn't exist
 * 2. Generate an auth token for the database
 * 3. Setup bidirectional sync
 * 4. Enable auto-sync every 60 seconds
 */
async function main() {
  console.log("=== Simple Turso Cloud Sync Example ===\n");

  // Check for required environment variable
  if (!process.env.TURSO_API_TOKEN) {
    console.error("Error: TURSO_API_TOKEN environment variable is required");
    console.error("Get your token from: https://turso.tech/app");
    process.exit(1);
  }

  // Open AgentFS with Turso Cloud sync
  // This will create a database named 'simple-agent' in Turso Cloud
  console.log("Opening AgentFS with Turso Cloud sync...");
  const agent = await AgentFS.open({
    id: "simple-agent",
    sync: tursoSync(), // Uses all defaults from environment variables
  });

  console.log("✓ Connected to Turso Cloud\n");

  // Store some data
  console.log("Storing data locally...");
  await agent.kv.set("greeting", "Hello from AgentFS!");
  await agent.kv.set("timestamp", new Date().toISOString());
  await agent.kv.set("counter", 42);

  console.log("✓ Data stored locally\n");

  // Manual sync to cloud
  console.log("Syncing to Turso Cloud...");
  await agent.sync!.push();
  console.log("✓ Data pushed to cloud\n");

  // Check sync status
  const status = agent.sync!.getStatus();
  console.log("Sync status:", {
    state: status.state,
    lastSync: status.lastSync?.toISOString(),
  });

  console.log("\n=== Auto-sync is enabled ===");
  console.log("Your data will automatically sync every 60 seconds.");
  console.log("Try modifying the cloud database and run this again to see changes pulled down.\n");

  // Cleanup
  await agent.close();
  console.log("✓ Connection closed");
}

main().catch((error) => {
  console.error("Error:", error.message);
  process.exit(1);
});
