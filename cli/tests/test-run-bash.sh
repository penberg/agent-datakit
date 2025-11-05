#!/bin/sh
set -e

echo -n "TEST interactive bash session... "

TEST_DB="test_agent.db"

# Clean up any existing test database
rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"

# Initialize the database using agentfs init
cargo run -- init "$TEST_DB" > /dev/null 2>&1

# Run bash session: write a file and read it back (like README example)
output=$(cargo run -- run --mount type=sqlite,src="$TEST_DB",dst=/agent /bin/bash -c '
echo "hello from agent" > /agent/hello.txt
cat /agent/hello.txt
' 2>&1)

# Verify we got the expected output
echo "$output" | grep -q "hello from agent" || {
    echo "FAILED"
    echo "$output"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
}

# Cleanup
rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"

echo "OK"
