#!/bin/sh
set -e

echo -n "TEST syscalls... "

# Compile the test program using Makefile
make -C tests/syscall clean > /dev/null 2>&1
make -C tests/syscall > /dev/null 2>&1

TEST_DB="test_syscalls.db"

# Test 1: Bind mount
mkdir -p sandbox
echo "Hello from virtual FD!" > sandbox/test.txt

if ! output=$(cargo run -- run --mount type=bind,src=sandbox,dst=/sandbox tests/syscall/test-syscalls /sandbox 2>&1); then
    echo "FAILED (bind mount)"
    echo "Output was: $output"
    rm -rf sandbox
    exit 1
fi

echo "$output" | grep -q "All tests passed!" || {
    echo "FAILED (bind mount): 'All tests passed!' not found"
    echo "Output was: $output"
    rm -rf sandbox
    exit 1
}

if [ ! -f sandbox/output.txt ]; then
    echo "FAILED (bind mount): output.txt was not created"
    rm -rf sandbox
    exit 1
fi

rm -rf sandbox

# Test 2: SQLite VFS
rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"

# Initialize the database
cargo run -- init "$TEST_DB" > /dev/null 2>&1

# Populate with test file
cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent /bin/bash -c 'echo "Hello from virtual FD!" > /agent/test.txt' > /dev/null 2>&1

# Run the syscall tests
if ! output=$(cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent tests/syscall/test-syscalls /agent 2>&1); then
    echo "FAILED (sqlite)"
    echo "Output was: $output"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

echo "$output" | grep -q "All tests passed!" || {
    echo "FAILED (sqlite): 'All tests passed!' not found"
    echo "Output was: $output"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
}

# Verify output file was created (by reading it back)
if ! output=$(cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent /bin/cat /agent/output.txt 2>&1); then
    echo "FAILED (sqlite): output.txt was not created or readable"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"

echo "OK"
