import { Database } from '@tursodatabase/database';

// File types for mode field
const S_IFMT = 0o170000;   // File type mask
const S_IFREG = 0o100000;  // Regular file
const S_IFDIR = 0o040000;  // Directory
const S_IFLNK = 0o120000;  // Symbolic link

// Default permissions
const DEFAULT_FILE_MODE = S_IFREG | 0o644;  // Regular file, rw-r--r--
const DEFAULT_DIR_MODE = S_IFDIR | 0o755;   // Directory, rwxr-xr-x

export interface Stats {
  ino: number;
  mode: number;
  nlink: number;
  uid: number;
  gid: number;
  size: number;
  atime: number;
  mtime: number;
  ctime: number;
  isFile(): boolean;
  isDirectory(): boolean;
  isSymbolicLink(): boolean;
}

export class Filesystem {
  private db: Database;
  private initialized: Promise<void>;
  private rootIno: number = 1;

  constructor(db: Database) {
    this.db = db;
    this.initialized = this.initialize();
  }

  private async initialize(): Promise<void> {
    // Ensure database is connected
    try {
      await this.db.connect();
    } catch (error: any) {
      // Ignore "already connected" errors
      if (!error.message?.includes('already')) {
        throw error;
      }
    }

    // Create the inode table
    await this.db.exec(`
      CREATE TABLE IF NOT EXISTS fs_inode (
        ino INTEGER PRIMARY KEY AUTOINCREMENT,
        mode INTEGER NOT NULL,
        uid INTEGER NOT NULL DEFAULT 0,
        gid INTEGER NOT NULL DEFAULT 0,
        size INTEGER NOT NULL DEFAULT 0,
        atime INTEGER NOT NULL,
        mtime INTEGER NOT NULL,
        ctime INTEGER NOT NULL
      )
    `);

    // Create the directory entry table
    await this.db.exec(`
      CREATE TABLE IF NOT EXISTS fs_dentry (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        parent_ino INTEGER NOT NULL,
        ino INTEGER NOT NULL,
        UNIQUE(parent_ino, name)
      )
    `);

    // Create index for efficient path lookups
    await this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_fs_dentry_parent
      ON fs_dentry(parent_ino, name)
    `);

    // Create the data blocks table
    await this.db.exec(`
      CREATE TABLE IF NOT EXISTS fs_data (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        ino INTEGER NOT NULL,
        offset INTEGER NOT NULL,
        size INTEGER NOT NULL,
        data BLOB NOT NULL
      )
    `);

    // Create index for efficient data block lookups
    await this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_fs_data_ino_offset
      ON fs_data(ino, offset)
    `);

    // Create the symlink table
    await this.db.exec(`
      CREATE TABLE IF NOT EXISTS fs_symlink (
        ino INTEGER PRIMARY KEY,
        target TEXT NOT NULL
      )
    `);

    // Create root directory if it doesn't exist
    await this.ensureRoot();
  }

  /**
   * Ensure root directory exists
   */
  private async ensureRoot(): Promise<void> {
    const stmt = this.db.prepare('SELECT ino FROM fs_inode WHERE ino = ?');
    const root = await stmt.get(this.rootIno);

    if (!root) {
      const now = Math.floor(Date.now() / 1000);
      const insertStmt = this.db.prepare(`
        INSERT INTO fs_inode (ino, mode, uid, gid, size, atime, mtime, ctime)
        VALUES (?, ?, 0, 0, 0, ?, ?, ?)
      `);
      await insertStmt.run(this.rootIno, DEFAULT_DIR_MODE, now, now, now);
    }
  }

  /**
   * Normalize a path
   */
  private normalizePath(path: string): string {
    // Remove trailing slashes except for root
    const normalized = path.replace(/\/+$/, '') || '/';
    // Ensure leading slash
    return normalized.startsWith('/') ? normalized : '/' + normalized;
  }

  /**
   * Split path into components
   */
  private splitPath(path: string): string[] {
    const normalized = this.normalizePath(path);
    if (normalized === '/') return [];
    return normalized.split('/').filter(p => p);
  }

  /**
   * Resolve a path to an inode number
   */
  private async resolvePath(path: string): Promise<number | null> {
    await this.initialized;

    const normalized = this.normalizePath(path);

    // Root directory
    if (normalized === '/') {
      return this.rootIno;
    }

    const parts = this.splitPath(normalized);
    let currentIno = this.rootIno;

    // Traverse the path
    for (const name of parts) {
      const stmt = this.db.prepare(`
        SELECT ino FROM fs_dentry
        WHERE parent_ino = ? AND name = ?
      `);
      const result = await stmt.get(currentIno, name) as { ino: number } | undefined;

      if (!result) {
        return null;
      }

      currentIno = result.ino;
    }

    return currentIno;
  }

  /**
   * Get parent directory inode and basename from path
   */
  private async resolveParent(path: string): Promise<{ parentIno: number; name: string } | null> {
    const normalized = this.normalizePath(path);

    if (normalized === '/') {
      return null; // Root has no parent
    }

    const parts = this.splitPath(normalized);
    const name = parts[parts.length - 1];
    const parentPath = parts.length === 1 ? '/' : '/' + parts.slice(0, -1).join('/');

    const parentIno = await this.resolvePath(parentPath);

    if (parentIno === null) {
      return null;
    }

    return { parentIno, name };
  }

  /**
   * Create an inode
   */
  private async createInode(mode: number, uid: number = 0, gid: number = 0): Promise<number> {
    const now = Math.floor(Date.now() / 1000);
    const stmt = this.db.prepare(`
      INSERT INTO fs_inode (mode, uid, gid, size, atime, mtime, ctime)
      VALUES (?, ?, ?, 0, ?, ?, ?)
    `);
    const result = await stmt.run(mode, uid, gid, now, now, now);
    return Number(result.lastInsertRowid);
  }

  /**
   * Create a directory entry
   */
  private async createDentry(parentIno: number, name: string, ino: number): Promise<void> {
    const stmt = this.db.prepare(`
      INSERT INTO fs_dentry (name, parent_ino, ino)
      VALUES (?, ?, ?)
    `);
    await stmt.run(name, parentIno, ino);
  }

  /**
   * Ensure parent directories exist
   */
  private async ensureParentDirs(path: string): Promise<void> {
    const parts = this.splitPath(path);

    // Remove the filename, keep only directory parts
    parts.pop();

    let currentIno = this.rootIno;
    let currentPath = '';

    for (const name of parts) {
      currentPath += '/' + name;

      // Check if this directory exists
      const stmt = this.db.prepare(`
        SELECT ino FROM fs_dentry
        WHERE parent_ino = ? AND name = ?
      `);
      const result = await stmt.get(currentIno, name) as { ino: number } | undefined;

      if (!result) {
        // Create directory
        const dirIno = await this.createInode(DEFAULT_DIR_MODE);
        await this.createDentry(currentIno, name, dirIno);
        currentIno = dirIno;
      } else {
        currentIno = result.ino;
      }
    }
  }

  /**
   * Get link count for an inode
   */
  private async getLinkCount(ino: number): Promise<number> {
    const stmt = this.db.prepare('SELECT COUNT(*) as count FROM fs_dentry WHERE ino = ?');
    const result = await stmt.get(ino) as { count: number };
    return result.count;
  }

  async writeFile(path: string, content: string | Buffer): Promise<void> {
    await this.initialized;

    // Ensure parent directories exist
    await this.ensureParentDirs(path);

    // Check if file already exists
    const ino = await this.resolvePath(path);

    if (ino !== null) {
      // Update existing file
      await this.updateFileContent(ino, content);
    } else {
      // Create new file
      const parent = await this.resolveParent(path);
      if (!parent) {
        throw new Error(`ENOENT: parent directory does not exist: ${path}`);
      }

      // Create inode
      const fileIno = await this.createInode(DEFAULT_FILE_MODE);

      // Create directory entry
      await this.createDentry(parent.parentIno, parent.name, fileIno);

      // Write content
      await this.updateFileContent(fileIno, content);
    }
  }

  private async updateFileContent(ino: number, content: string | Buffer): Promise<void> {
    const buffer = typeof content === 'string' ? Buffer.from(content, 'utf-8') : content;
    const now = Math.floor(Date.now() / 1000);

    // Delete existing data blocks
    const deleteStmt = this.db.prepare('DELETE FROM fs_data WHERE ino = ?');
    await deleteStmt.run(ino);

    // Write data in chunks (for now, single chunk, but can be extended)
    const stmt = this.db.prepare(`
      INSERT INTO fs_data (ino, offset, size, data)
      VALUES (?, ?, ?, ?)
    `);
    await stmt.run(ino, 0, buffer.length, buffer);

    // Update inode size and mtime
    const updateStmt = this.db.prepare(`
      UPDATE fs_inode
      SET size = ?, mtime = ?
      WHERE ino = ?
    `);
    await updateStmt.run(buffer.length, now, ino);
  }

  async readFile(path: string): Promise<string> {
    await this.initialized;

    const ino = await this.resolvePath(path);
    if (ino === null) {
      throw new Error(`ENOENT: no such file or directory, open '${path}'`);
    }

    // Get all data blocks
    const stmt = this.db.prepare(`
      SELECT data FROM fs_data
      WHERE ino = ?
      ORDER BY offset ASC
    `);
    const rows = await stmt.all(ino) as { data: Buffer }[];

    if (rows.length === 0) {
      return '';
    }

    // Concatenate all chunks
    const buffers = rows.map(row => row.data);
    const combined = Buffer.concat(buffers);

    // Update atime
    const now = Math.floor(Date.now() / 1000);
    const updateStmt = this.db.prepare('UPDATE fs_inode SET atime = ? WHERE ino = ?');
    await updateStmt.run(now, ino);

    return combined.toString('utf-8');
  }

  async readdir(path: string): Promise<string[]> {
    await this.initialized;

    const ino = await this.resolvePath(path);
    if (ino === null) {
      throw new Error(`ENOENT: no such file or directory, scandir '${path}'`);
    }

    // Get all directory entries
    const stmt = this.db.prepare(`
      SELECT name FROM fs_dentry
      WHERE parent_ino = ?
      ORDER BY name ASC
    `);
    const rows = await stmt.all(ino) as { name: string }[];

    return rows.map(row => row.name);
  }

  async deleteFile(path: string): Promise<void> {
    await this.initialized;

    const ino = await this.resolvePath(path);
    if (ino === null) {
      throw new Error(`ENOENT: no such file or directory, unlink '${path}'`);
    }

    const parent = await this.resolveParent(path);
    if (!parent) {
      throw new Error(`Cannot delete root directory`);
    }

    // Delete the directory entry
    const stmt = this.db.prepare(`
      DELETE FROM fs_dentry
      WHERE parent_ino = ? AND name = ?
    `);
    await stmt.run(parent.parentIno, parent.name);

    // Check if this was the last link to the inode
    const linkCount = await this.getLinkCount(ino);
    if (linkCount === 0) {
      // Delete the inode and all associated data (CASCADE will handle data blocks)
      const deleteStmt = this.db.prepare('DELETE FROM fs_inode WHERE ino = ?');
      await deleteStmt.run(ino);
    }
  }

  async stat(path: string): Promise<Stats> {
    await this.initialized;

    const ino = await this.resolvePath(path);
    if (ino === null) {
      throw new Error(`ENOENT: no such file or directory, stat '${path}'`);
    }

    const stmt = this.db.prepare(`
      SELECT ino, mode, uid, gid, size, atime, mtime, ctime
      FROM fs_inode
      WHERE ino = ?
    `);
    const row = await stmt.get(ino) as {
      ino: number;
      mode: number;
      uid: number;
      gid: number;
      size: number;
      atime: number;
      mtime: number;
      ctime: number;
    } | undefined;

    if (!row) {
      throw new Error(`Inode not found: ${ino}`);
    }

    const nlink = await this.getLinkCount(ino);

    return {
      ino: row.ino,
      mode: row.mode,
      nlink: nlink,
      uid: row.uid,
      gid: row.gid,
      size: row.size,
      atime: row.atime,
      mtime: row.mtime,
      ctime: row.ctime,
      isFile: () => (row.mode & S_IFMT) === S_IFREG,
      isDirectory: () => (row.mode & S_IFMT) === S_IFDIR,
      isSymbolicLink: () => (row.mode & S_IFMT) === S_IFLNK,
    };
  }

  /**
   * Wait for initialization to complete
   */
  async ready(): Promise<void> {
    await this.initialized;
  }
}
