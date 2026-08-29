#!/usr/bin/env sh
set -eu

package_dir="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
repository_dir="$(CDPATH='' cd -- "$package_dir/../.." && pwd)"
native_dir="$package_dir/native"

mkdir -p "$native_dir"

build_macos() {
  rustup target add aarch64-apple-darwin x86_64-apple-darwin

  cargo build --locked --release --manifest-path "$repository_dir/Cargo.toml" \
    --target aarch64-apple-darwin -p sponsor-shell
  install -m 755 \
    "$repository_dir/target/aarch64-apple-darwin/release/sponsor-shell" \
    "$native_dir/sponsor-shell-aarch64-apple-darwin"

  cargo build --locked --release --manifest-path "$repository_dir/Cargo.toml" \
    --target x86_64-apple-darwin -p sponsor-shell
  install -m 755 \
    "$repository_dir/target/x86_64-apple-darwin/release/sponsor-shell" \
    "$native_dir/sponsor-shell-x86_64-apple-darwin"
}

build_linux() {
  target="$1"
  platform="$2"

  docker run --rm --platform "$platform" \
    -e CARGO_TARGET_DIR=/tmp/moneymux-target \
    -e MONEYMUX_HOST_UID="$(id -u)" \
    -e MONEYMUX_HOST_GID="$(id -g)" \
    -v "$repository_dir:/workspace" \
    -w /workspace \
    rust:1.96-bookworm@sha256:a339861ae23e9abb272cea45dfafde21760d2ce6577a70f8a926153677902663 \
    sh -c "cargo build --locked --release -p sponsor-shell && \
      install -m 755 /tmp/moneymux-target/release/sponsor-shell \
        /workspace/packages/sponsor-shell/native/sponsor-shell-$target && \
      chown \"\$MONEYMUX_HOST_UID:\$MONEYMUX_HOST_GID\" \
        /workspace/packages/sponsor-shell/native/sponsor-shell-$target"
}

case "$(uname -s)" in
  Darwin) build_macos ;;
  *)
    echo "The release builder currently expects macOS for Apple targets." >&2
    exit 1
    ;;
esac

command -v docker >/dev/null 2>&1 || {
  echo "Docker is required to build the Linux release executables." >&2
  exit 1
}

build_linux x86_64-unknown-linux-gnu linux/amd64
build_linux aarch64-unknown-linux-gnu linux/arm64

node "$package_dir/scripts/validate-package.mjs"
