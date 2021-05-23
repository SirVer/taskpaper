#!/usr/bin/env bash
# Script for building your rust projects.
set -ex

source ci/common.bash

# $1 {path} = Path to cross/cargo executable
CROSS=$1
# $1 {string} = <Target Triple> e.g. x86_64-pc-windows-msvc
TARGET_TRIPLE=$2
# $3 {boolean} = Are we building for deployment?
RELEASE_BUILD=$3

required_arg $CROSS 'CROSS'
required_arg $TARGET_TRIPLE '<Target Triple>'

if [ -z "$RELEASE_BUILD" ]; then
    $CROSS build --all --target $TARGET_TRIPLE
    $CROSS build --all --target $TARGET_TRIPLE --all-features
else
    $CROSS build -p taskpaper_cli --target $TARGET_TRIPLE --release
fi
