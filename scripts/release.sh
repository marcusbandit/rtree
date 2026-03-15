#!/usr/bin/env bash
set -euo pipefail

# Check if running as root/sudo
if [ "$EUID" -eq 0 ] || [ -n "${SUDO_USER:-}" ]; then
    echo "ERROR: Do not run this with sudo."
    exit 1
fi

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    echo "Usage: ./scripts/release.sh <version>"
    echo "Example: ./scripts/release.sh 0.2.0"
    exit 1
fi

# Strip leading 'v' if given
VERSION="${VERSION#v}"
TAG="v$VERSION"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== Releasing rtree $TAG ==="
echo ""

# Check working tree is clean (except for what we're about to change)
if [ -n "$(git status --porcelain | grep -v '^\?\?')" ]; then
    echo "ERROR: Working tree has uncommitted changes. Commit or stash first."
    git status --short
    exit 1
fi

# Bump version in Cargo.toml
echo "[1/7] Bumping version in Cargo.toml..."
sed -i "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml

# Update Cargo.lock
echo "[2/7] Updating Cargo.lock..."
cargo update --workspace --quiet

# Build to make sure it compiles
echo "[3/7] Building release binary..."
cargo build --release --quiet

# Generate completions (needed for .deb metadata check)
echo "[4/7] Generating completions..."
mkdir -p completions
./target/release/rtree --generate-completions bash > completions/rtree.bash
./target/release/rtree --generate-completions zsh  > completions/_rtree
./target/release/rtree --generate-completions fish > completions/rtree.fish

# Commit, tag, push
echo "[5/7] Committing and tagging $TAG..."
git add Cargo.toml Cargo.lock PKGBUILD completions/
git commit -m "release $TAG"
git tag "$TAG"

echo "[6/7] Pushing to GitHub..."
git push origin main
git push origin "$TAG"

# Wait for GitHub to have the tarball, then get sha256
echo "[7/7] Fetching tarball sha256 from GitHub..."
TARBALL_URL="https://github.com/marcusbandit/rtree/archive/refs/tags/$TAG.tar.gz"
for i in $(seq 1 12); do
    SHA=$(curl -sL "$TARBALL_URL" | sha256sum | awk '{print $1}')
    if [ -n "$SHA" ] && [ "$SHA" != "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" ]; then
        break
    fi
    echo "  Waiting for tarball to be available... ($i/12)"
    sleep 5
done

# Update PKGBUILD
echo ""
echo "=== Updating PKGBUILD ==="
sed -i "s/^pkgver=.*/pkgver=$VERSION/" PKGBUILD
sed -i "s/sha256sums=('.*')/sha256sums=('$SHA')/" PKGBUILD

git add PKGBUILD
git commit -m "update PKGBUILD for $TAG"
git push origin main

echo ""
echo "=== Done! ==="
echo ""
echo "GitHub Actions is now building binaries for all platforms."
echo "Check progress at: https://github.com/marcusbandit/rtree/actions"
echo ""
echo "To push to AUR, run:"
echo ""
echo "  cd /tmp && rm -rf aur-rtree && \\"
echo "  git clone ssh://aur@aur.archlinux.org/rtree.git aur-rtree && \\"
echo "  cp $ROOT/PKGBUILD aur-rtree/ && \\"
echo "  cd aur-rtree && \\"
echo "  makepkg --printsrcinfo > .SRCINFO && \\"
echo "  git add PKGBUILD .SRCINFO && \\"
echo "  git commit -m \"Update to $TAG\" && \\"
echo "  git push origin HEAD:master"
echo ""
