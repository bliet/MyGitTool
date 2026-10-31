#!/usr/bin/env bash
# Builds a release binary and packages it for the current platform into
# dist/: a tar.gz on Linux, an app bundle in a .dmg on macOS, a zip on
# Windows. Run it from the repository root, e.g. `scripts/package.sh`.
set -euo pipefail

# `cargo pkgid` prints e.g. `path+file:///…/crates/app#0.1.0` or `…#app@0.1.0`.
pkgid=$(cargo pkgid -p app)
version=${pkgid##*[#@]}

cargo build --release --locked -p app

dist=dist
rm -rf "$dist"
mkdir -p "$dist"

case "$(uname -s)" in
  Linux)
    name="mygittool-$version-linux-$(uname -m)"
    stage="$dist/$name"
    mkdir -p "$stage"
    cp target/release/mygittool packaging/mygittool.desktop packaging/icons/mygittool.png \
      README.md LICENSE "$stage/"
    tar -C "$dist" -czf "$dist/$name.tar.gz" "$name"
    rm -rf "$stage"
    ;;
  Darwin)
    name="mygittool-$version-macos-$(uname -m)"
    app="$dist/stage/MyGitTool.app"
    mkdir -p "$app/Contents/MacOS"
    cp target/release/mygittool "$app/Contents/MacOS/"
    sed "s/@VERSION@/$version/g" packaging/Info.plist > "$app/Contents/Info.plist"
    # The app icon, from the 1024 px rendering with the system's tools.
    iconset="$dist/mygittool.iconset"
    mkdir -p "$iconset" "$app/Contents/Resources"
    for size in 16 32 128 256 512; do
      sips -z "$size" "$size" packaging/icons/mygittool-1024.png \
        --out "$iconset/icon_${size}x${size}.png" > /dev/null
      double=$((size * 2))
      sips -z "$double" "$double" packaging/icons/mygittool-1024.png \
        --out "$iconset/icon_${size}x${size}@2x.png" > /dev/null
    done
    iconutil -c icns "$iconset" -o "$app/Contents/Resources/mygittool.icns"
    rm -rf "$iconset"
    cp README.md LICENSE "$dist/stage/"
    hdiutil create -volname MyGitTool -srcfolder "$dist/stage" -ov -format UDZO "$dist/$name.dmg"
    rm -rf "$dist/stage"
    ;;
  MINGW* | MSYS* | CYGWIN*)
    name="mygittool-$version-windows-x86_64"
    stage="$dist/$name"
    mkdir -p "$stage"
    cp target/release/mygittool.exe README.md LICENSE "$stage/"
    (cd "$dist" && 7z a -tzip "$name.zip" "$name" > /dev/null)
    rm -rf "$stage"
    ;;
  *)
    echo "unsupported platform: $(uname -s)" >&2
    exit 1
    ;;
esac

ls -l "$dist"
