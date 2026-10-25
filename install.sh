#!/bin/sh
# Install the newest `ve` release on macOS or Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/binderg/snapcut-core/main/install.sh | sh
#
# VE_INSTALL_DIR picks the folder (default ~/.local/bin). VE_VERSION installs
# a given tag (v0.1.0-rc.1) instead of the newest release. FFmpeg is not
# included: install it yourself (brew install ffmpeg, apt install ffmpeg).
set -eu

REPO="binderg/snapcut-core"
DIR="${VE_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  *) echo "no ve build for $(uname -s) $(uname -m); build from source with cargo" >&2; exit 1 ;;
esac

if [ -n "${VE_VERSION:-}" ]; then
  url="https://github.com/$REPO/releases/download/$VE_VERSION/ve-$target.tar.gz"
else
  url="https://github.com/$REPO/releases/latest/download/ve-$target.tar.gz"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "downloading $url"
curl -fsSL "$url" -o "$tmp/ve.tar.gz"
tar xzf "$tmp/ve.tar.gz" -C "$tmp"
mkdir -p "$DIR"
install -m 755 "$tmp/ve-$target/ve" "$DIR/ve"
echo "installed $DIR/ve"

case ":$PATH:" in
  *":$DIR:"*) ;;
  *) echo "add $DIR to your PATH, for example: echo 'export PATH=\"$DIR:\$PATH\"' >> ~/.profile" ;;
esac
command -v ffmpeg > /dev/null 2>&1 || echo "ve needs FFmpeg: install it (brew install ffmpeg, or apt install ffmpeg)"
"$DIR/ve" version
