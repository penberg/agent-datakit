import { AgentFS, tursoSync } from "agentfs-sdk";

/**
 * Advanced Turso Cloud Sync Example
 *
 * This example demonstrates custom configuration options for Turso Cloud sync:
 * - Explicit organization and API token
 * - Custom database name
 * - Disabled auto-sync (manual control only)
 * - Custom sync interval
 *
 * Use cases:
 * - Production environments with specific requirements
 * - Testing different sync strategies
 * - Connecting to existing databases
 * - Multiple agents with different sync policies
 */
async function main() {
  console.log("=== Advanced Turso Cloud Sync Example ===\n");

  // Example 1: Custom configuration with manual sync
  console.log("Example 1: Manual sync control\n");

  const agent1 = await AgentFS.open({
    id: "agent-manual",
    sync: tursoSync({
      org: process.env.TURSO_API_ORG || "default",
      apiToken: process.env.TURSO_API_TOKEN!,
      databaseUrl: "agent-manual-db", // Custom database name
      autoSync: false, // Disable auto-sync
    }),
  });

  console.log("✓ Agent with manual sync created");

  // Store data
  await agent1.kv.set("mode", "manual");
  await agent1.kv.set("sync_strategy", "on-demand");

  console.log("✓ Data stored locally");

  // Manually control when to sync
  console.log("Pushing changes to cloud...");
  await agent1.sync!.push();
  console.log("✓ Changes pushed\n");

  await agent1.close();

  // Example 2: Custom sync interval
  console.log("Example 2: Fast auto-sync (every 10 seconds)\n");

  const agent2 = await AgentFS.open({
    id: "agent-fast-sync",
    sync: tursoSync({
      databaseUrl: "agent-fast-sync-db",
      autoSync: true,
      interval: 10000, // Sync every 10 seconds
    }),
  });

  console.log("✓ Agent with fast auto-sync created");

  await agent2.kv.set("sync_interval", "10s");
  await agent2.kv.set("use_case", "real-time collaboration");

  console.log("✓ Data stored (will auto-sync every 10 seconds)");

  // Demonstrate sync operations
  console.log("\nDemonstrating sync operations:");

  // Pull (get latest from cloud)
  console.log("- Pulling latest changes...");
  await agent2.sync!.pull();
  console.log("  ✓ Pull complete");

  // Push (send local changes to cloud)
  console.log("- Pushing local changes...");
  await agent2.sync!.push();
  console.log("  ✓ Push complete");

  // Sync (bidirectional - pull + push)
  console.log("- Bidirectional sync...");
  await agent2.sync!.sync();
  console.log("  ✓ Sync complete");

  // Check status
  const status = agent2.sync!.getStatus();
  console.log("\nSync status:", {
    state: status.state,
    lastSync: status.lastSync?.toISOString(),
    lastError: status.lastError || "none",
  });

  await agent2.close();

  console.log("\n=== Examples Complete ===");
  console.log("\nConfiguration options summary:");
  console.log("- org: Turso organization (env: TURSO_API_ORG)");
  console.log("- apiToken: API token (env: TURSO_API_TOKEN)");
  console.log("- databaseUrl: Database name or URL (env: TURSO_DATABASE_URL)");
  console.log("- autoSync: Enable/disable auto-sync (env: TURSO_AUTO_SYNC)");
  console.log("- interval: Sync interval in ms (env: TURSO_SYNC_INTERVAL)");
}

main().catch((error) => {
  console.error("Error:", error.message);
  process.exit(1);
});
