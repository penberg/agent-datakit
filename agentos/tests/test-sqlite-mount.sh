#!/bin/sh
set -e

echo -n "TEST SQLite VFS mount... "

TEST_DB="test_agent.db"

# Clean up any existing test database
rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"

# Initialize the database schema
timeout 5 cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent /bin/true 2>/dev/null || true

# Populate the test database
sqlite3 "$TEST_DB" <<EOF
-- Create /test directory (ino=2)
INSERT INTO fs_inode (ino, mode, uid, gid, size, atime, mtime, ctime)
VALUES (2, 16877, 0, 0, 0, 1234567890, 1234567890, 1234567890);

INSERT INTO fs_dentry (name, parent_ino, ino)
VALUES ('test', 1, 2);

-- Create /readme.txt file (ino=3)
INSERT INTO fs_inode (ino, mode, uid, gid, size, atime, mtime, ctime)
VALUES (3, 33188, 0, 0, 43, 1234567890, 1234567890, 1234567890);

INSERT INTO fs_dentry (name, parent_ino, ino)
VALUES ('readme.txt', 1, 3);

INSERT INTO fs_data (ino, offset, size, data)
VALUES (3, 0, 43, 'This is a test file in the root directory.');

-- Create /test/hello.txt file (ino=4)
INSERT INTO fs_inode (ino, mode, uid, gid, size, atime, mtime, ctime)
VALUES (4, 33188, 0, 0, 23, 1234567890, 1234567890, 1234567890);

INSERT INTO fs_dentry (name, parent_ino, ino)
VALUES ('hello.txt', 2, 4);

INSERT INTO fs_data (ino, offset, size, data)
VALUES (4, 0, 23, 'Hello from SQLite VFS!');
EOF

# Test 1: Directory existence
if ! cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent /bin/bash -c "ls -d /agent" > /dev/null 2>&1; then
    echo "FAILED: Cannot access /agent directory"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

# Test 2: List root directory
if ! output=$(cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent ls /agent 2>&1); then
    echo "FAILED: Cannot list /agent directory"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

echo "$output" | grep -q "readme.txt" || {
    echo "FAILED: 'readme.txt' not found in root directory"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
}

# Test 3: List subdirectory
if ! output=$(cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent ls /agent/test 2>&1); then
    echo "FAILED: Cannot list /agent/test directory"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

echo "$output" | grep -q "hello.txt" || {
    echo "FAILED: 'hello.txt' not found in /agent/test"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
}

# Test 4: Read root file
if ! output=$(cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent cat /agent/readme.txt 2>&1); then
    echo "FAILED: Cannot read /agent/readme.txt"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

echo "$output" | grep -q "test file in the root directory" || {
    echo "FAILED: Incorrect content in /agent/readme.txt"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
}

# Test 5: Read subdirectory file
if ! output=$(cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent cat /agent/test/hello.txt 2>&1); then
    echo "FAILED: Cannot read /agent/test/hello.txt"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

echo "$output" | grep -q "Hello from SQLite VFS" || {
    echo "FAILED: Incorrect content in /agent/test/hello.txt"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
}

# Test 6: Stat file
if ! cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent stat /agent/readme.txt > /dev/null 2>&1; then
    echo "FAILED: Cannot stat /agent/readme.txt"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

# Test 7: Test -f operator
if ! cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent /bin/bash -c "test -f /agent/readme.txt && echo 'is-file'" 2>&1 | grep -q "is-file"; then
    echo "FAILED: test -f operator failed"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

# Test 8: Test -d operator
if ! cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent /bin/bash -c "test -d /agent/test && echo 'is-dir'" 2>&1 | grep -q "is-dir"; then
    echo "FAILED: test -d operator failed"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

# Cleanup
rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"

echo "OK"
