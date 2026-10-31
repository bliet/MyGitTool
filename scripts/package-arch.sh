#!/usr/bin/env bash
# Builds an Arch Linux package into dist/, to install with
# `sudo pacman -U dist/mygittool-*.pkg.tar.zst`. Run it as a normal user on
# Arch or a derivative; extra arguments go to makepkg (e.g. --syncdeps).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
# `cargo pkgid` prints e.g. `path+file:///…/crates/app#0.1.0`. A pacman
# version cannot contain '-', so a pre-release like 1.0.0-beta becomes
# 1.0.0_beta.
pkgid=$(cd "$root" && cargo pkgid -p app)
version=${pkgid##*[#@]}
version=${version//-/_}

build=$(mktemp -d)
trap 'rm -rf "$build"' EXIT
sed "s/^pkgver=.*/pkgver=$version/" "$root/packaging/arch/PKGBUILD" > "$build/PKGBUILD"
mkdir -p "$root/dist"
(cd "$build" && MYGITTOOL_ROOT="$root" PKGDEST="$root/dist" makepkg --force --cleanbuild "$@")
ls -l "$root"/dist/*.pkg.tar.zst
