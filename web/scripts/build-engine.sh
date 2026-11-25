#!/bin/sh
# Builds the Board engine (bindings/wasm, kition-board-kit) to WebAssembly into public/engine, where the
# editor loads it from its own package, and copies the fonts it measures text with. Needs wasm-pack.
set -eu
web="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$web/.." && pwd)"
if toolchain="$(rustc --print sysroot 2>/dev/null)" && [ -d "$toolchain/lib" ]; then
  export DYLD_FALLBACK_LIBRARY_PATH="$toolchain/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
fi
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}"
# A dependency that compiles C for the target needs a WebAssembly toolchain: Apple's clang and ar cannot
# build WebAssembly archives, so use Homebrew's LLVM when it is there and no compiler is set.
if [ -z "${CC_wasm32_unknown_unknown:-}" ] && command -v brew >/dev/null 2>&1; then
  for formula in llvm@21 llvm; do
    llvm="$(brew --prefix "$formula" 2>/dev/null || true)"
    if [ -n "$llvm" ] && [ -x "$llvm/bin/clang" ]; then
      export CC_wasm32_unknown_unknown="$llvm/bin/clang" AR_wasm32_unknown_unknown="$llvm/bin/llvm-ar"
      break
    fi
  done
fi
cd "$repo"
wasm-pack build bindings/wasm --target web --release --no-typescript --out-dir "$web/public/engine"
rm -f "$web/public/engine/.gitignore" "$web/public/engine/package.json"
# The engine lays text out with the faces the page draws it in: Inter first (Droid Sans Fallback has
# no Latin letters), then Droid Sans Fallback for Chinese and Japanese, both from the kition-fonts
# checkout next to the repository (or KITION_FONTS_DIR). The editor hands them to the engine's
# addFont and to the page's FontFace before the board opens.
fonts="${KITION_FONTS_DIR:-$repo/../kition-fonts}/fonts"
rm -rf "$web/public/fonts"
if [ -f "$fonts/inter/Inter-Variable.ttf" ] && [ -f "$fonts/droid-sans-fallback/DroidSansFallbackFull.ttf" ]; then
  mkdir -p "$web/public/fonts"
  cp "$fonts/inter/Inter-Variable.ttf" "$web/public/fonts/"
  cp "$fonts/inter/OFL.txt" "$web/public/fonts/Inter-OFL.txt"
  cp "$fonts/droid-sans-fallback/DroidSansFallbackFull.ttf" "$web/public/fonts/"
  cp "$fonts/droid-sans-fallback/NOTICE" "$web/public/fonts/DroidSansFallback-NOTICE.txt"
else
  echo "warning: no kition-fonts checkout at $fonts; text is measured by estimate and Chinese shows as boxes" >&2
fi
