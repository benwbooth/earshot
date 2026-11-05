#!/usr/bin/env bash
# Publishes Earshot to GitHub and starts its first release, in one go:
#   1. creates github.com/benwbooth/earshot (public) and pushes the main branch
#   2. makes a signing key for the Flatpak repository (GitHub keeps the private
#      half as a secret; the public half is committed so people can verify it)
#   3. turns on GitHub Pages, built by the Pages workflow
#   4. tags v<version from Cargo.toml>, which starts the Release workflow:
#      Windows installer + portable zip, macOS dmg + Homebrew cask, AppImage,
#      Flatpak bundle + Flatpak repository on GitHub Pages
# Safe to run again: steps that are already done are skipped.
set -euo pipefail
cd "$(dirname "$0")/.."

repo=benwbooth/earshot
site=https://benwbooth.github.io/earshot/
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
tag="v$version"

git branch -M main

# Flatpak signing key
private_key=""
if [ ! -f packaging/flatpak/earshot-flatpak.gpg ]; then
    GNUPGHOME=$(mktemp -d)
    export GNUPGHOME
    gpg --batch --passphrase '' --quick-gen-key "Earshot Flatpak repository" rsa4096 sign never
    gpg --batch --export > packaging/flatpak/earshot-flatpak.gpg
    private_key=$(gpg --batch --armor --export-secret-keys)
    git add packaging/flatpak/earshot-flatpak.gpg
    git commit -m "Public key of the Flatpak repository's signing key"
fi

# Repository
if ! gh repo view "$repo" >/dev/null 2>&1; then
    gh repo create "$repo" --public --homepage "$site" \
        --description "A modern desktop music player (Rust + Qt Quick): charts, Reddit radio, YouTube albums and playlists. A port of Headset."
fi
git remote get-url origin >/dev/null 2>&1 || git remote add origin "https://github.com/$repo.git"
git push -u origin main

if [ -n "$private_key" ]; then
    printf '%s' "$private_key" | gh secret set FLATPAK_GPG_PRIVATE_KEY --repo "$repo"
fi

# GitHub Pages, published by .github/workflows/pages.yml
gh api -X POST "repos/$repo/pages" -f build_type=workflow >/dev/null 2>&1 \
    || gh api -X PUT "repos/$repo/pages" -f build_type=workflow >/dev/null
gh workflow run pages.yml --repo "$repo" --ref main || true

# First release
if ! git rev-parse "$tag" >/dev/null 2>&1; then
    git tag -a "$tag" -m "Earshot $version"
fi
git push origin "$tag"

echo
echo "Release $tag is building: https://github.com/$repo/actions"
echo "When it finishes, the downloads are at https://github.com/$repo/releases/tag/$tag"
