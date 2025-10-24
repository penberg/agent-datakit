import { AgentOS } from "agentos-sdk";

async function main() {
  const agent = new AgentOS(":memory:");

  // Write a file
  console.log("Writing file...");
  await agent.fs.writeFile("/documents/readme.txt", "Hello, world!");

  // Read the file
  console.log("\nReading file...");
  const content = await agent.fs.readFile("/documents/readme.txt");
  console.log("Content:", content);

  // Get file stats
  console.log("\nFile stats:");
  const stats = await agent.fs.stat("/documents/readme.txt");
  console.log("  Inode:", stats.ino);
  console.log("  Size:", stats.size, "bytes");
  console.log("  Mode:", stats.mode.toString(8));
  console.log("  Links:", stats.nlink);
  console.log("  Is file:", stats.isFile());
  console.log("  Is directory:", stats.isDirectory());
  console.log("  Created:", new Date(stats.ctime * 1000).toISOString());
  console.log("  Modified:", new Date(stats.mtime * 1000).toISOString());

  // List directory
  console.log("\nListing /documents:");
  const files = await agent.fs.readdir("/documents");
  console.log("  Files:", files);

  // Write more files
  await agent.fs.writeFile("/documents/notes.txt", "Some notes");
  await agent.fs.writeFile("/images/photo.jpg", "binary data here");

  // List root
  console.log("\nListing /:");
  const rootFiles = await agent.fs.readdir("/");
  console.log("  Directories:", rootFiles);

  // Check directory stats
  console.log("\nDirectory stats for /documents:");
  const dirStats = await agent.fs.stat("/documents");
  console.log("  Is directory:", dirStats.isDirectory());
  console.log("  Mode:", dirStats.mode.toString(8));
}

main().catch(console.error);