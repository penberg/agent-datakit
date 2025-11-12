import { AgentFS } from "agentfs-sdk";

async function main() {
  const agentfs = await AgentFS.create(":memory:");

  // Write a file
  console.log("Writing file...");
  await agentfs.fs.writeFile("/documents/readme.txt", "Hello, world!");

  // Read the file
  console.log("\nReading file...");
  const content = await agentfs.fs.readFile("/documents/readme.txt");
  console.log("Content:", content);

  // Get file stats
  console.log("\nFile stats:");
  const stats = await agentfs.fs.stat("/documents/readme.txt");
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
  const files = await agentfs.fs.readdir("/documents");
  console.log("  Files:", files);

  // Write more files
  await agentfs.fs.writeFile("/documents/notes.txt", "Some notes");
  await agentfs.fs.writeFile("/images/photo.jpg", "binary data here");

  // List root
  console.log("\nListing /:");
  const rootFiles = await agentfs.fs.readdir("/");
  console.log("  Directories:", rootFiles);

  // Check directory stats
  console.log("\nDirectory stats for /documents:");
  const dirStats = await agentfs.fs.stat("/documents");
  console.log("  Is directory:", dirStats.isDirectory());
  console.log("  Mode:", dirStats.mode.toString(8));
}

main().catch(console.error);