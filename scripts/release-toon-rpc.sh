#!/bin/bash
# Release script for @reddb-io/toon-rpc
#
# Usage: ./release.sh [patch|minor|major]
#
# Bumps version, builds, runs dry-run publish, and asks for confirmation

set -e

VERSION_TYPE=${1:-patch}
PACKAGE_DIR="$(dirname "$0")/packages/toon-rpc"

echo "📦 Releasing @reddb-io/toon-rpc"
echo "================================"
echo ""

cd "$PACKAGE_DIR"

# Get current version
CURRENT=$(node -p "require('./package.json').version")
echo "Current version: $CURRENT"

# Bump version
npm version "$VERSION_TYPE" --no-git-tag-version
NEW=$(node -p "require('./package.json').version")
echo "New version: $NEW"
echo ""

# Build
echo "🔨 Building..."
pnpm build

# Dry run
echo ""
echo "🔍 Dry-run publish:"
npm publish --dry-run

echo ""
echo "📝 Files to be published:"
npm pack --dry-run

echo ""
echo "Ready to publish? Run:"
echo "  cd packages/toon-rpc && npm publish --access public"
