#!/usr/bin/env bash
# Check an ffmpeg + ffprobe pair in folder $1 before we ship it: it runs, is
# not a GPL build, and has every encoder an export format needs
# (ve_render::Format and ffmpeg::h264_encoder).
set -euo pipefail

ext=""
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) ext=.exe ;; esac
ff="$1/ffmpeg$ext"
"$1/ffprobe$ext" -hide_banner -version | head -1
"$ff" -hide_banner -version | head -1

conf=$("$ff" -hide_banner -buildconf)
for bad in --enable-gpl --enable-nonfree; do
  if grep -q -- "$bad" <<< "$conf"; then
    echo "this ffmpeg was configured with $bad; ve may only ship LGPL builds" >&2
    exit 1
  fi
done

encoders=$("$ff" -hide_banner -encoders)
for e in libopenh264 libvpx-vp9 libopus libmp3lame prores_ks aac flac pcm_s16le; do
  if ! grep -q -E "^ [A-Z.]{6} $e( |$)" <<< "$encoders"; then
    echo "this ffmpeg has no $e encoder" >&2
    exit 1
  fi
done
echo "ok: LGPL, all export encoders present"
