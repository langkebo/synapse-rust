#!/bin/bash
export PATH="/home/hula/.rustup/toolchains/1.93.0-x86_64-unknown-linux-gnu/bin:$PATH"
cargo check 2>&1 | grep -E "^error\[|^  -->|^\[30m\^$|^\[0m$" | sed 's/\x1b\[[0-9;]*m//g' | sed 's/\^$//'
