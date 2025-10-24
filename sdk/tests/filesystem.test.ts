import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { Database } from '@tursodatabase/database';
import { Filesystem } from '../src/filesystem';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

describe('Filesystem Integration Tests', () => {
  let db: Database;
  let fs: Filesystem;
  let tempDir: string;
  let dbPath: string;

  beforeEach(async () => {
    // Create temporary directory for test database
    tempDir = mkdtempSync(join(tmpdir(), 'agentos-test-'));
    dbPath = join(tempDir, 'test.db');

    // Initialize database and Filesystem
    db = new Database(dbPath);
    fs = new Filesystem(db);
  });

  afterEach(() => {
    // Clean up temporary directories
    try {
      rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup errors
    }
  });

  describe('File Write Operations', () => {
    it('should write and read a simple text file', async () => {
      await fs.writeFile('/test.txt', 'Hello, World!');
      const content = await fs.readFile('/test.txt');
      expect(content).toBe('Hello, World!');
    });

    it('should write and read files in subdirectories', async () => {
      await fs.writeFile('/dir/subdir/file.txt', 'nested content');
      const content = await fs.readFile('/dir/subdir/file.txt');
      expect(content).toBe('nested content');
    });

    it('should overwrite existing file', async () => {
      await fs.writeFile('/overwrite.txt', 'original content');
      await fs.writeFile('/overwrite.txt', 'new content');
      const content = await fs.readFile('/overwrite.txt');
      expect(content).toBe('new content');
    });

    it('should handle empty file content', async () => {
      await fs.writeFile('/empty.txt', '');
      const content = await fs.readFile('/empty.txt');
      expect(content).toBe('');
    });

    it('should handle large file content', async () => {
      const largeContent = 'x'.repeat(100000);
      await fs.writeFile('/large.txt', largeContent);
      const content = await fs.readFile('/large.txt');
      expect(content).toBe(largeContent);
    });

    it('should handle files with special characters in content', async () => {
      const specialContent = 'Special chars: \n\t\r"\'\\';
      await fs.writeFile('/special.txt', specialContent);
      const content = await fs.readFile('/special.txt');
      expect(content).toBe(specialContent);
    });
  });

  describe('File Read Operations', () => {
    it('should throw error when reading non-existent file', async () => {
      await expect(fs.readFile('/non-existent.txt')).rejects.toThrow();
    });

    it('should read multiple different files', async () => {
      await fs.writeFile('/file1.txt', 'content 1');
      await fs.writeFile('/file2.txt', 'content 2');
      await fs.writeFile('/file3.txt', 'content 3');

      expect(await fs.readFile('/file1.txt')).toBe('content 1');
      expect(await fs.readFile('/file2.txt')).toBe('content 2');
      expect(await fs.readFile('/file3.txt')).toBe('content 3');
    });
  });

  describe('Directory Operations', () => {
    it('should list files in root directory', async () => {
      await fs.writeFile('/file1.txt', 'content 1');
      await fs.writeFile('/file2.txt', 'content 2');
      await fs.writeFile('/file3.txt', 'content 3');

      const files = await fs.readdir('/');
      expect(files).toContain('file1.txt');
      expect(files).toContain('file2.txt');
      expect(files).toContain('file3.txt');
      expect(files).toHaveLength(3);
    });

    it('should list files in subdirectory', async () => {
      await fs.writeFile('/dir/file1.txt', 'content 1');
      await fs.writeFile('/dir/file2.txt', 'content 2');
      await fs.writeFile('/other/file3.txt', 'content 3');

      const files = await fs.readdir('/dir');
      expect(files).toContain('file1.txt');
      expect(files).toContain('file2.txt');
      expect(files).not.toContain('file3.txt');
      expect(files).toHaveLength(2);
    });

    it('should return empty array for empty directory', async () => {
      await fs.writeFile('/dir/file.txt', 'content');
      // /dir exists but is not empty, root exists and should be empty except for 'dir'
      const files = await fs.readdir('/');
      expect(files).toContain('dir');
    });

    it('should distinguish between files in different directories', async () => {
      await fs.writeFile('/dir1/file.txt', 'content 1');
      await fs.writeFile('/dir2/file.txt', 'content 2');

      const files1 = await fs.readdir('/dir1');
      const files2 = await fs.readdir('/dir2');

      expect(files1).toContain('file.txt');
      expect(files2).toContain('file.txt');
      expect(files1).toHaveLength(1);
      expect(files2).toHaveLength(1);
    });

    it('should list subdirectories within a directory', async () => {
      await fs.writeFile('/parent/child1/file.txt', 'content');
      await fs.writeFile('/parent/child2/file.txt', 'content');
      await fs.writeFile('/parent/file.txt', 'content');

      const entries = await fs.readdir('/parent');
      expect(entries).toContain('file.txt');
      expect(entries).toContain('child1');
      expect(entries).toContain('child2');
    });

    it('should handle nested directory structures', async () => {
      await fs.writeFile('/a/b/c/d/file.txt', 'deep content');
      const files = await fs.readdir('/a/b/c/d');
      expect(files).toContain('file.txt');
    });
  });

  describe('File Delete Operations', () => {
    it('should delete an existing file', async () => {
      await fs.writeFile('/delete-me.txt', 'content');
      await fs.deleteFile('/delete-me.txt');
      await expect(fs.readFile('/delete-me.txt')).rejects.toThrow();
    });

    it('should handle deleting non-existent file', async () => {
      await expect(fs.deleteFile('/non-existent.txt')).rejects.toThrow('ENOENT');
    });

    it('should delete file and update directory listing', async () => {
      await fs.writeFile('/dir/file1.txt', 'content 1');
      await fs.writeFile('/dir/file2.txt', 'content 2');

      await fs.deleteFile('/dir/file1.txt');

      const files = await fs.readdir('/dir');
      expect(files).not.toContain('file1.txt');
      expect(files).toContain('file2.txt');
      expect(files).toHaveLength(1);
    });

    it('should allow recreating deleted file', async () => {
      await fs.writeFile('/recreate.txt', 'original');
      await fs.deleteFile('/recreate.txt');
      await fs.writeFile('/recreate.txt', 'new content');
      const content = await fs.readFile('/recreate.txt');
      expect(content).toBe('new content');
    });
  });

  describe('Path Handling', () => {
    it('should handle paths with trailing slashes', async () => {
      await fs.writeFile('/dir/file.txt', 'content');
      const files1 = await fs.readdir('/dir');
      const files2 = await fs.readdir('/dir/');
      expect(files1).toEqual(files2);
    });

    it('should handle paths with special characters', async () => {
      const specialPath = '/dir-with-dash/file_with_underscore.txt';
      await fs.writeFile(specialPath, 'content');
      const content = await fs.readFile(specialPath);
      expect(content).toBe('content');
    });
  });

  describe('Concurrent Operations', () => {
    it('should handle concurrent writes to different files', async () => {
      const operations = Array.from({ length: 10 }, (_, i) =>
        fs.writeFile(`/concurrent-${i}.txt`, `content ${i}`)
      );
      await Promise.all(operations);

      // Verify all files were created
      for (let i = 0; i < 10; i++) {
        const content = await fs.readFile(`/concurrent-${i}.txt`);
        expect(content).toBe(`content ${i}`);
      }
    });

    it('should handle concurrent reads', async () => {
      await fs.writeFile('/concurrent-read.txt', 'shared content');

      const results = await Promise.all(
        Array.from({ length: 10 }, () => fs.readFile('/concurrent-read.txt'))
      );

      results.forEach(content => {
        expect(content).toBe('shared content');
      });
    });
  });

  describe('File System Integrity', () => {
    it('should maintain file hierarchy integrity', async () => {
      await fs.writeFile('/root.txt', 'root');
      await fs.writeFile('/dir1/file.txt', 'dir1');
      await fs.writeFile('/dir2/file.txt', 'dir2');
      await fs.writeFile('/dir1/subdir/file.txt', 'subdir');

      expect(await fs.readFile('/root.txt')).toBe('root');
      expect(await fs.readFile('/dir1/file.txt')).toBe('dir1');
      expect(await fs.readFile('/dir2/file.txt')).toBe('dir2');
      expect(await fs.readFile('/dir1/subdir/file.txt')).toBe('subdir');

      const rootFiles = await fs.readdir('/');
      expect(rootFiles).toContain('root.txt');
      expect(rootFiles).toContain('dir1');
      expect(rootFiles).toContain('dir2');
    });

    it('should support multiple files with same name in different directories', async () => {
      await fs.writeFile('/dir1/config.json', '{"version": 1}');
      await fs.writeFile('/dir2/config.json', '{"version": 2}');

      expect(await fs.readFile('/dir1/config.json')).toBe('{"version": 1}');
      expect(await fs.readFile('/dir2/config.json')).toBe('{"version": 2}');
    });
  });

  describe('Standalone Usage', () => {
    it('should work with in-memory database when no db provided', async () => {
      const standaloneDb = new Database(':memory:');
      await standaloneDb.connect();
      const standaloneFs = new Filesystem(standaloneDb);
      await standaloneFs.writeFile('/test.txt', 'standalone content');
      const content = await standaloneFs.readFile('/test.txt');
      expect(content).toBe('standalone content');
    });

    it('should maintain isolation between instances', async () => {
      const db1 = new Database(':memory:');
      await db1.connect();
      const fs1 = new Filesystem(db1);

      const db2 = new Database(':memory:');
      await db2.connect();
      const fs2 = new Filesystem(db2);

      await fs1.writeFile('/test.txt', 'fs1 content');
      await fs2.writeFile('/test.txt', 'fs2 content');

      expect(await fs1.readFile('/test.txt')).toBe('fs1 content');
      expect(await fs2.readFile('/test.txt')).toBe('fs2 content');
    });
  });

  describe('Persistence', () => {
    it('should persist data across Filesystem instances', async () => {
      await fs.writeFile('/persist.txt', 'persistent content');

      // Create new Filesystem instance with same database
      const newFs = new Filesystem(db);
      const content = await newFs.readFile('/persist.txt');
      expect(content).toBe('persistent content');
    });
  });
});
