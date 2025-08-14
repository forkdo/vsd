#!/usr/bin/env bash
set -e

# GitHub Actions 会自动把 inputs.version 转成 INPUT_VERSION 环境变量
VERSION_INPUT="${INPUT_VERSION}"

# Detect OS and architecture
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$ARCH" in
  x86_64) ARCH="x86_64" ;;
  aarch64 | arm64) ARCH="aarch64" ;;
  *) echo "Unsupported architecture: $ARCH" && exit 1 ;;
esac

# Determine Zig version & download URL
if [[ -z "$VERSION_INPUT" ]]; then
  VERSION=$(curl -s https://ziglang.org/download/index.json | jq -r '.latest.version')
elif [[ "$VERSION_INPUT" == "dev" || "$VERSION_INPUT" == "master" ]]; then
  VERSION="master"
else
  VERSION="$VERSION_INPUT"
fi

PACKAGES_DIR="$HOME/zig"
mkdir -p "$PACKAGES_DIR"

if [[ "$VERSION" == "master" ]]; then
  URL=$(curl -s https://ziglang.org/download/index.json \
    | jq -r ".master.\"${ARCH}-${OS}\".tarball")
else
  URL=$(curl -s https://ziglang.org/download/index.json \
    | jq -r ".\"$VERSION\".\"${ARCH}-${OS}\".tarball")
fi

echo "Downloading Zig $VERSION for $ARCH-$OS from $URL"
curl -L "$URL" | tar xJC "$PACKAGES_DIR" --strip-components=1

echo "$PACKAGES_DIR" >> $GITHUB_PATH
echo "Zig $VERSION installed successfully."
zig version
