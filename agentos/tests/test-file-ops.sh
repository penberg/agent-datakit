#!/bin/sh
set -e

echo -n "TEST file-ops... "

# Compile the test program
gcc -o tests/test_fd tests/test_fd.c

# Create test directory and file
mkdir -p sandbox
echo "Hello from virtual FD!" > sandbox/test.txt

# Run the test through agentos
if ! output=$(cargo run -- run --mount type=bind,src=sandbox,dst=/sandbox tests/test_fd 2>&1); then
    echo "Test FAILED: Command failed to run"
    echo "Output was: $output"
    rm -rf sandbox
    exit 1
fi

# Check if the output file was created
if [ ! -f sandbox/output.txt ]; then
    echo "Test FAILED: output.txt was not created"
    echo "Output was: $output"
    rm -rf sandbox
    exit 1
fi

# Check the content
if ! grep -q "Written via virtual FD" sandbox/output.txt; then
    echo "Test FAILED: output.txt doesn't contain expected content"
    echo "Output was: $output"
    cat sandbox/output.txt
    rm -rf sandbox
    exit 1
fi

# Check that all tests passed message is in output
echo "$output" | grep -q "All tests passed!" || {
    echo "Test FAILED: 'All tests passed!' not found in output"
    echo "Output was: $output"
    rm -rf sandbox
    exit 1
}

# Cleanup
rm -rf sandbox

echo "OK"
