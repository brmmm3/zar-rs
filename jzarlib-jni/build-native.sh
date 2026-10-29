#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
PROJECT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
JAVA_HOME=${JAVA_HOME:-$(dirname "$(dirname "$(readlink -f "$(command -v javac)")")")}
OS=$(uname -s)

cargo build --manifest-path "$ROOT/Cargo.toml" -p czarlib
mkdir -p "$PROJECT/target/native"

case "$OS" in
    Linux)
        JNI_PLATFORM=linux
        LIBRARY=libzarlib_jni.so
        LINK_FLAGS="-shared -Wl,-rpath,\$ORIGIN/../../../target/debug"
        ;;
    Darwin)
        JNI_PLATFORM=darwin
        LIBRARY=libzarlib_jni.dylib
        LINK_FLAGS="-dynamiclib -Wl,-rpath,@loader_path/../../../target/debug"
        ;;
    *)
        echo "Unsupported JNI build platform: $OS" >&2
        exit 1
        ;;
esac

# shellcheck disable=SC2086
cc -std=c11 -O2 -Wall -Wextra -Werror -fPIC $LINK_FLAGS \
    -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/$JNI_PLATFORM" \
    -I"$ROOT/czarlib/include" \
    "$PROJECT/src/main/c/jzarlib_jni.c" \
    -L"$ROOT/target/debug" -lczarlib \
    -o "$PROJECT/target/native/$LIBRARY"
printf 'Built %s\n' "$PROJECT/target/native/$LIBRARY"
