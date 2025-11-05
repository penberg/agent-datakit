#!/bin/sh
set -e

echo -n "TEST init... "

# Cleanup any existing agent.db
rm -f agent.db agent.db-shm agent.db-wal

# Test: Run init command
if ! output=$(cargo run -- init 2>&1); then
    echo "FAILED: init command failed"
    echo "Output was: $output"
    exit 1
fi

# Check that agent.db was created
if [ ! -f agent.db ]; then
    echo "FAILED: agent.db was not created"
    echo "Output was: $output"
    exit 1
fi

# Check that output contains success message
echo "$output" | grep -q "Created agent filesystem: agent.db" || {
    echo "FAILED: Expected success message not found in output"
    echo "Output was: $output"
    rm -f agent.db agent.db-shm agent.db-wal
    exit 1
}

# Test: Running init again should fail without --force
if cargo run -- init 2>&1 | grep -q "already exists"; then
    : # Expected behavior
else
    echo "FAILED: init should fail when agent.db already exists"
    rm -f agent.db agent.db-shm agent.db-wal
    exit 1
fi

# Test: Running init with --force should succeed
if ! output=$(cargo run -- init --force 2>&1); then
    echo "FAILED: init --force command failed"
    echo "Output was: $output"
    rm -f agent.db agent.db-shm agent.db-wal
    exit 1
fi

# Check that output contains success message
echo "$output" | grep -q "Created agent filesystem: agent.db" || {
    echo "FAILED: Expected success message not found in init --force output"
    echo "Output was: $output"
    rm -f agent.db agent.db-shm agent.db-wal
    exit 1
}

# Cleanup
rm -f agent.db agent.db-shm agent.db-wal

echo "OK"
