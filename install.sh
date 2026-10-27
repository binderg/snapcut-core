#!/bin/sh
# Install the newest `ve` release on macOS or Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/binderg/snapcut-core/main/install.sh | sh
#
# The release folder (ve, its bundled LGPL ffmpeg + ffprobe, skills) goes in
# VE_HOME (default ~/.local/share/ve) and a `ve` link in VE_INSTALL_DIR
# (default ~/.local/bin), so the bundled ffmpeg never lands on your PATH.
# VE_VERSION installs a given tag (v0.1.0-rc.1, or nightly) instead of the
# newest release.
set -eu

REPO="binderg/snapcut-core"
DIR="${VE_INSTALL_DIR:-$HOME/.local/bin}"
HOME_DIR="${VE_HOME:-$HOME/.local/share/ve}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=x86_64-unknown-linux-gnu ext=tar.gz ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ext=tar.gz ;;
  Darwin-arm64) target=aarch64-apple-darwin ext=zip ;;
  Darwin-x86_64) target=x86_64-apple-darwin ext=zip ;;
  *) echo "no ve build for $(uname -s) $(uname -m); build from source with cargo" >&2; exit 1 ;;
esac

if [ -n "${VE_VERSION:-}" ]; then
  url="https://github.com/$REPO/releases/download/$VE_VERSION/ve-$target.$ext"
else
  url="https://github.com/$REPO/releases/latest/download/ve-$target.$ext"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "downloading $url"
curl -fsSL "$url" -o "$tmp/ve.$ext"
case "$ext" in
  zip) unzip -q "$tmp/ve.$ext" -d "$tmp" ;;
  *) tar xzf "$tmp/ve.$ext" -C "$tmp" ;;
esac

# The next ve command replaces any daemon still running the old binary.
rm -rf "$HOME_DIR"
mkdir -p "$(dirname "$HOME_DIR")" "$DIR"
mv "$tmp/ve-$target" "$HOME_DIR"
ln -sf "$HOME_DIR/ve" "$DIR/ve"
echo "installed $HOME_DIR (ve, ffmpeg, ffprobe) and linked $DIR/ve"

case ":$PATH:" in
  *":$DIR:"*) ;;
  *) echo "add $DIR to your PATH, for example: echo 'export PATH=\"$DIR:\$PATH\"' >> ~/.profile" ;;
esac
"$DIR/ve" version
