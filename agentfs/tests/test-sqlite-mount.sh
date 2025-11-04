#!/bin/sh
set -e

echo -n "TEST SQLite VFS mount... "

TEST_DB="test_agent.db"

# Clean up any existing test database
rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"

# Initialize the database schema (creates empty filesystem)
timeout 5 cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent /bin/true 2>/dev/null || true

# Create /readme.txt file
cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent /bin/bash -c "echo 'This is a test file in the root directory.' > /agent/readme.txt" > /dev/null 2>&1

# Create /hello.txt file
cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent /bin/bash -c "echo 'Hello from SQLite VFS!' > /agent/hello.txt" > /dev/null 2>&1

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

# Test 3: Check hello.txt in root
if ! output=$(cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent ls /agent 2>&1); then
    echo "FAILED: Cannot list /agent directory"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

echo "$output" | grep -q "hello.txt" || {
    echo "FAILED: 'hello.txt' not found in /agent"
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

# Test 5: Read hello.txt file
if ! output=$(cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent cat /agent/hello.txt 2>&1); then
    echo "FAILED: Cannot read /agent/hello.txt"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

echo "$output" | grep -q "Hello from SQLite VFS" || {
    echo "FAILED: Incorrect content in /agent/hello.txt"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
}

# Test 6: Test -f operator
if ! cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent /bin/bash -c "test -f /agent/readme.txt && echo 'is-file'" 2>&1 | grep -q "is-file"; then
    echo "FAILED: test -f operator failed"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

# Test 8: Test -d operator on root
if ! cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent /bin/bash -c "test -d /agent && echo 'is-dir'" 2>&1 | grep -q "is-dir"; then
    echo "FAILED: test -d operator failed"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

# Cleanup
rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"

echo "OK"
