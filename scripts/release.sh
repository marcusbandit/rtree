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
        read -rp "Enter version (e.g. 0.2.0): " NEW_VER
        NEW_VER="${NEW_VER#v}"
        read -rp "pkgrel [1]: " NEW_REL
        NEW_REL="${NEW_REL:-1}"
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
    echo "This is a patch release — only the PKGBUILD changes (pkgrel bump)."
    echo "No new git tag or binary build."
else
    echo "This will tag $TAG and trigger a full GitHub Actions build."
fi
echo ""
read -rp "Release ${NEW_VER}-${NEW_REL}? [y/N]: " CONFIRM
if [[ ! "$CONFIRM" =~ ^[Yy]$ ]]; then
    echo "Cancelled."
    exit 0
fi

echo ""

# Check working tree is clean
if [ -n "$(git status --porcelain | grep -v '^\?\?')" ]; then
    echo "ERROR: Working tree has uncommitted changes. Commit or stash first."
    git status --short
    exit 1
fi

if $PKGBUILD_ONLY; then
    # Patch: just bump pkgrel in PKGBUILD, no Cargo version change
    echo "[1/3] Bumping pkgrel in PKGBUILD..."
    sed -i "s/^pkgrel=.*/pkgrel=${NEW_REL}/" PKGBUILD

    echo "[2/3] Committing..."
    git add PKGBUILD
    git commit -m "release ${NEW_VER}-${NEW_REL} (pkgrel bump)"

    echo "[3/3] Done. Push when ready."
else
    echo "[1/6] Bumping version in Cargo.toml to $NEW_VER..."
    sed -i "s/^version = \".*\"/version = \"${NEW_VER}\"/" Cargo.toml

    echo "[2/6] Updating Cargo.lock..."
    cargo update --workspace --quiet

    echo "[3/6] Building release binary..."
    cargo build --release --quiet

    echo "[4/6] Generating completions..."
    mkdir -p completions
    ./target/release/rtree --generate-completions bash > completions/rtree.bash
    ./target/release/rtree --generate-completions zsh  > completions/_rtree
    ./target/release/rtree --generate-completions fish > completions/rtree.fish

    echo "[5/6] Committing and tagging $TAG..."
    git add Cargo.toml Cargo.lock completions/
    git commit -m "release $TAG"
    git tag "$TAG"

    echo "[6/6] Fetching tarball sha256 (push to GitHub first to get it)..."
    echo ""
    echo "  Push when ready: git push origin main && git push origin $TAG"
    echo "  Then run this to update PKGBUILD:"
    echo ""
    echo "  SHA=\$(curl -sL https://github.com/marcusbandit/rtree/archive/refs/tags/$TAG.tar.gz | sha256sum | awk '{print \$1}')"
    echo "  sed -i \"s/^pkgver=.*/pkgver=${NEW_VER}/\" PKGBUILD"
    echo "  sed -i \"s/^pkgrel=.*/pkgrel=${NEW_REL}/\" PKGBUILD"
    echo "  sed -i \"s/sha256sums=('.*')/sha256sums=('\$SHA')/\" PKGBUILD"
    echo ""
fi

echo ""
echo "To push to AUR after pushing to GitHub:"
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
