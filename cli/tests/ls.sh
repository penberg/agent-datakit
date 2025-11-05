#!/bin/sh
set -e

echo -n "TEST ls... "

mkdir -p sandbox
touch sandbox/hello.txt

if ! output=$(cargo run -- run --mount type=bind,src=sandbox,dst=/sandbox ls /sandbox 2>&1); then
    echo "Test FAILED: Command failed to run"
    echo "Output was: $output"
    rm -rf sandbox
    exit 1
fi

rm -rf sandbox

echo "$output" | grep -q "hello.txt" || {
    echo "Test FAILED: 'hello.txt' not found in output"
    echo "Output was: $output"
    exit 1
}

echo "OK"
