#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
PROJECT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
JAVA_HOME=${JAVA_HOME:-$(dirname "$(dirname "$(readlink -f "$(command -v javac)")")")}
OS=$(uname -s)

if [ "$#" -gt 1 ]; then
    echo "Usage: $0 [--skip-czarlib-build]" >&2
    exit 1
fi
case "${1:-}" in
    "") cargo build --manifest-path "$ROOT/Cargo.toml" --release -p czarlib ;;
    --skip-czarlib-build) ;;
    *) echo "Usage: $0 [--skip-czarlib-build]" >&2; exit 1 ;;
esac
mkdir -p "$PROJECT/target/native"

case "$OS" in
    Linux)
        JNI_PLATFORM=linux
        LIBRARY=libzarlib_jni.so
        CZARLIB=libczarlib.so
        LINK_FLAGS="-shared -Wl,-rpath,\$ORIGIN/../../../target/release"
        ;;
    Darwin)
        JNI_PLATFORM=darwin
        LIBRARY=libzarlib_jni.dylib
        CZARLIB=libczarlib.dylib
        LINK_FLAGS="-dynamiclib -Wl,-rpath,@loader_path -Wl,-rpath,@loader_path/../../../target/release"
        ;;
    *)
        echo "Unsupported JNI build platform: $OS" >&2
        exit 1
        ;;
esac

[ -f "$ROOT/target/release/$CZARLIB" ] \
    || { echo "error: prebuilt czarlib library is missing" >&2; exit 1; }

# shellcheck disable=SC2086
cc -std=c11 -O2 -Wall -Wextra -Werror -fPIC $LINK_FLAGS \
    -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/$JNI_PLATFORM" \
    -I"$ROOT/czarlib/include" \
    "$PROJECT/src/main/c/jzarlib_jni.c" \
    -L"$ROOT/target/release" -lczarlib \
    -o "$PROJECT/target/native/$LIBRARY"

if [ "$OS" = Darwin ]; then
    CZARLIB_ID=$(otool -D "$ROOT/target/release/$CZARLIB" | sed -n '2p')
    [ -n "$CZARLIB_ID" ] || { echo "error: czarlib has no install name" >&2; exit 1; }
    install_name_tool -change "$CZARLIB_ID" "@rpath/$CZARLIB" "$PROJECT/target/native/$LIBRARY"
fi

printf 'Built %s\n' "$PROJECT/target/native/$LIBRARY"
