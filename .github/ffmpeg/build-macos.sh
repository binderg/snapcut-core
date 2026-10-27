#!/usr/bin/env bash
# Build a static LGPL ffmpeg + ffprobe for macOS from the sources pinned in
# pins.env, into the folder given as $1. Run it on the target architecture
# (an arm64 runner for aarch64, an Intel runner for x86_64).
#
# Everything is linked statically except Apple's own system libraries, so the
# binaries run on any Mac with macOS 11 or newer and nothing from Homebrew.
set -euo pipefail

out=$(mkdir -p "$1" && cd "$1" && pwd)
here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=pins.env
source "$here/pins.env"

export MACOSX_DEPLOYMENT_TARGET=11.0
jobs=$(sysctl -n hw.ncpu)
work=$(mktemp -d)
prefix="$work/prefix"
mkdir -p "$prefix"
# Only our own .pc files, so nothing from Homebrew gets linked in.
export PKG_CONFIG_LIBDIR="$prefix/lib/pkgconfig"

brew install nasm pkgconf

fetch() { # url sha256 dir
  curl -fsSL --retry 3 "$1" -o "$work/src.tar.gz"
  echo "$2  $work/src.tar.gz" | shasum -a 256 -c -
  mkdir -p "$work/$3"
  tar xzf "$work/src.tar.gz" -C "$work/$3" --strip-components 1
}

fetch "$LAME_URL" "$LAME_SHA256" lame
(
  cd "$work/lame"
  # 3.100 exports a symbol it no longer defines; Apple's linker refuses it.
  sed -i '' '/lame_init_old/d' include/libmp3lame.sym
  ./configure --prefix="$prefix" --enable-static --disable-shared --disable-frontend --disable-dependency-tracking
  make -j"$jobs" install
)

fetch "$OPUS_URL" "$OPUS_SHA256" opus
(
  cd "$work/opus"
  ./configure --prefix="$prefix" --enable-static --disable-shared --disable-doc --disable-extra-programs
  make -j"$jobs" install
)

fetch "$VPX_URL" "$VPX_SHA256" vpx
(
  cd "$work/vpx"
  ./configure --prefix="$prefix" --enable-static --disable-shared --enable-pic --enable-vp9-highbitdepth \
    --disable-examples --disable-tools --disable-docs --disable-unit-tests
  make -j"$jobs" install
)

fetch "$OPENH264_URL" "$OPENH264_SHA256" openh264
(
  cd "$work/openh264"
  make -j"$jobs" PREFIX="$prefix" install-static
)

fetch "$FFMPEG_SRC_URL" "$FFMPEG_SRC_SHA256" ffmpeg
(
  cd "$work/ffmpeg"
  # --disable-autodetect, then name every library: anything else found on the
  # runner (Homebrew's xz, SDL, ...) would be linked as a dylib users lack.
  ./configure --prefix="$prefix" \
    --pkg-config-flags=--static \
    --extra-cflags="-I$prefix/include" --extra-ldflags="-L$prefix/lib" \
    --disable-autodetect --disable-shared --enable-static \
    --disable-ffplay --disable-doc --disable-debug \
    --enable-videotoolbox --enable-audiotoolbox --enable-zlib --enable-bzlib --enable-iconv \
    --enable-libmp3lame --enable-libopus --enable-libvpx --enable-libopenh264
  make -j"$jobs"
  cp ffmpeg ffprobe "$out/"
  cp COPYING.LGPLv2.1 "$out/LICENSE.txt"
)

# Fail if anything outside the OS got linked dynamically.
for bin in "$out/ffmpeg" "$out/ffprobe"; do
  if otool -L "$bin" | tail -n +2 | grep -v -E '^\s+(/System/Library/|/usr/lib/)'; then
    echo "$bin links a library macOS does not ship" >&2
    exit 1
  fi
done
rm -rf "$work"
