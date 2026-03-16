#!/usr/bin/env bash
set -euo pipefail

if [ "$EUID" -eq 0 ] || [ -n "${SUDO_USER:-}" ]; then
    echo "ERROR: Do not run this with sudo."
    exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Read current version and pkgrel
CARGO_VER=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
PKGREL=$(grep '^pkgrel=' PKGBUILD | sed 's/pkgrel=//')

IFS='.' read -r MAJOR MINOR PATCH <<< "$CARGO_VER"
CURRENT="${CARGO_VER}-${PKGREL}"

PATCH_VER="${CARGO_VER}"
PATCH_REL="$((PKGREL + 1))"

MINOR_VER="${MAJOR}.${MINOR}.$((PATCH + 1))"
MINOR_REL="1"

MAJOR_VER="${MAJOR}.$((MINOR + 1)).0"
MAJOR_REL="1"

echo ""
echo "Current version: $CURRENT"
echo ""
echo "  [1] patch  →  ${PATCH_VER}-${PATCH_REL}   (PKGBUILD only, no new tag)"
echo "  [2] minor  →  ${MINOR_VER}-${MINOR_REL}"
echo "  [3] major  →  ${MAJOR_VER}-${MAJOR_REL}"
echo "  [4] custom"
echo ""
read -rp "Bump? [1/2/3/4]: " CHOICE

case "$CHOICE" in
    1)
        NEW_VER="$PATCH_VER"
        NEW_REL="$PATCH_REL"
        PKGBUILD_ONLY=true
        ;;
    2)
        NEW_VER="$MINOR_VER"
        NEW_REL="$MINOR_REL"
        PKGBUILD_ONLY=false
        ;;
    3)
        NEW_VER="$MAJOR_VER"
        NEW_REL="$MAJOR_REL"
        PKGBUILD_ONLY=false
        ;;
    4)
        read -rp "Enter version (e.g. 0.2.0 or 0.2.0-1): " INPUT
        INPUT="${INPUT#v}"
        if [[ "$INPUT" == *-* ]]; then
            NEW_VER="${INPUT%-*}"
            NEW_REL="${INPUT##*-}"
        else
            NEW_VER="$INPUT"
            read -rp "pkgrel [1]: " NEW_REL
            NEW_REL="${NEW_REL:-1}"
        fi
        PKGBUILD_ONLY=false
        ;;
    *)
        echo "Cancelled."
        exit 0
        ;;
esac

TAG="v${NEW_VER}"

echo ""
if $PKGBUILD_ONLY; then
    echo "Patch release — pkgrel bump only, no new tag or binary build."
else
    echo "This will tag $TAG, push to GitHub, update PKGBUILD, and print the AUR command."
fi
echo ""
read -rp "Release ${NEW_VER}-${NEW_REL}? [y/N]: " CONFIRM
if [[ ! "$CONFIRM" =~ ^[Yy]$ ]]; then
    echo "Cancelled."
    exit 0
fi

echo ""

# Check working tree is clean
if [ -n "$(git status --porcelain | grep -vF '??')" ]; then
    echo "ERROR: Working tree has uncommitted changes. Commit or stash first."
    git status --short
    exit 1
fi

if $PKGBUILD_ONLY; then
    echo "[1/4] Bumping pkgrel in PKGBUILD..."
    sed -i "s/^pkgrel=.*/pkgrel=${NEW_REL}/" PKGBUILD

    echo "[2/4] Committing..."
    git add PKGBUILD
    git commit -m "release ${NEW_VER}-${NEW_REL} (pkgrel bump)"

    echo "[3/4] Pushing to GitHub..."
    git push origin main

    echo "[4/4] Done."
else
    echo "[1/8] Bumping version in Cargo.toml to $NEW_VER..."
    sed -i "s/^version = .*/version = \"${NEW_VER}\"/" Cargo.toml

    echo "[2/8] Updating Cargo.lock..."
    cargo update --workspace --quiet

    echo "[3/8] Building release binary..."
    cargo build --release --quiet

    echo "[4/8] Generating completions..."
    mkdir -p completions
    ./target/release/rtree --generate-completions bash > completions/rtree.bash
    ./target/release/rtree --generate-completions zsh  > completions/_rtree
    ./target/release/rtree --generate-completions fish > completions/rtree.fish

    echo "[5/8] Committing and tagging $TAG..."
    git add Cargo.toml Cargo.lock completions/
    git commit -m "release $TAG"
    git tag "$TAG"

    echo "[6/8] Pushing to GitHub..."
    git push origin main
    git push origin "$TAG"

    echo "[7/8] Fetching tarball sha256..."
    TARBALL_URL="https://github.com/marcusbandit/rtree/archive/refs/tags/$TAG.tar.gz"
    SHA=""
    for i in $(seq 1 15); do
        SHA=$(curl -sL "$TARBALL_URL" | sha256sum | awk '{print $1}')
        if [ -n "$SHA" ] && [ "$SHA" != "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" ]; then
            break
        fi
        echo "  Waiting for tarball... ($i/15)"
        sleep 5
    done

    echo "[8/8] Updating and pushing PKGBUILD..."
    sed -i "s/^pkgver=.*/pkgver=${NEW_VER}/" PKGBUILD
    sed -i "s/^pkgrel=.*/pkgrel=${NEW_REL}/" PKGBUILD
    sed -i "s/sha256sums=('.*')/sha256sums=('${SHA}')/" PKGBUILD
    git add PKGBUILD
    git commit -m "update PKGBUILD for $TAG"
    git push origin main
fi

echo ""
echo "=== Done! GitHub is building binaries. ==="
echo "    https://github.com/marcusbandit/rtree/actions"
echo ""
echo "To push to AUR (needs SSH agent):"
echo ""
echo "  cd /tmp && rm -rf aur-rtree && \\"
echo "  git clone ssh://aur@aur.archlinux.org/rtree.git aur-rtree && \\"
echo "  cp $ROOT/PKGBUILD aur-rtree/ && \\"
echo "  cd aur-rtree && \\"
echo "  makepkg --printsrcinfo > .SRCINFO && \\"
echo "  git add PKGBUILD .SRCINFO && \\"
echo "  git commit -m \"release ${NEW_VER}-${NEW_REL}\" && \\"
echo "  git push origin HEAD:master"
echo ""
