#!/bin/sh

DIR="$(dirname "$0")"

"$DIR/test-init.sh"
"$DIR/ls.sh"
"$DIR/test-file-ops.sh"
"$DIR/test-mount.sh"
"$DIR/test-sqlite-mount.sh"
