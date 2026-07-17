#!/bin/bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
reference_root="$root/参考/iina"
reference_commit="45187444f8e1bd957a185aadcbdef58e583ed4ba"
sparkle_version="2.9.4"
sparkle_artifact_root="$reference_root/build/DerivedData/SourcePackages/artifacts/sparkle/Sparkle"
sparkle_target="$sparkle_artifact_root/Sparkle.xcframework"

if [[ ! -d "$reference_root/.git" ]]; then
  mkdir -p "$(dirname "$reference_root")"
  git clone --filter=blob:none --no-checkout https://github.com/iina/iina.git "$reference_root"
fi

git -C "$reference_root" fetch --depth 1 origin "$reference_commit"
git -C "$reference_root" checkout --detach --force "$reference_commit"

if [[ ! -f "$reference_root/deps/lib/libmpv.2.dylib" ]]; then
  (
    cd "$reference_root"
    bash other/download_libs.sh
  )
fi

if [[ ! -d "$sparkle_target" ]]; then
  archive="$RUNNER_TEMP/Sparkle-$sparkle_version.tar.xz"
  extracted="$RUNNER_TEMP/Sparkle-$sparkle_version"
  curl --fail --location --retry 3 \
    "https://github.com/sparkle-project/Sparkle/releases/download/$sparkle_version/Sparkle-$sparkle_version.tar.xz" \
    --output "$archive"
  rm -rf "$extracted"
  mkdir -p "$extracted"
  tar -xJf "$archive" -C "$extracted"
  source_xcframework="$(find "$extracted" -type d -name Sparkle.xcframework -print -quit)"
  if [[ -n "$source_xcframework" ]]; then
    mkdir -p "$sparkle_artifact_root"
    cp -R "$source_xcframework" "$sparkle_target"
  elif [[ -d "$extracted/Sparkle.framework" ]]; then
    mkdir -p "$sparkle_target/macos-arm64_x86_64"
    cp -R \
      "$extracted/Sparkle.framework" \
      "$sparkle_target/macos-arm64_x86_64/Sparkle.framework"
  else
    echo "Sparkle framework was not found in $archive" >&2
    exit 1
  fi
fi

test -f "$reference_root/Configs/Deployment.xcconfig"
test -f "$reference_root/deps/include/mpv/render.h"
test -f "$reference_root/deps/lib/libmpv.2.dylib"
test -d "$sparkle_target/macos-arm64_x86_64/Sparkle.framework"

echo "Prepared IINA $reference_commit and Sparkle $sparkle_version for macOS packaging."
