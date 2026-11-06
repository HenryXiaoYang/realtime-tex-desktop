#!/bin/sh
# Type-check the app for Windows and Linux from a Mac (nothing is linked, so stand-ins do for the
# Windows resource compiler and Linux's C headers). Needs:
#   rustup target add x86_64-pc-windows-msvc x86_64-unknown-linux-gnu
set -e
cd "$(dirname "$0")/.."
mkdir -p target/fake-rc
cat > target/fake-rc/llvm-rc <<'RC'
#!/bin/sh
case "$*" in *"/?"*) echo "LLVM Resource Converter"; echo "  -no-preprocess"; exit 0;; esac
while [ $# -gt 0 ]; do [ "$1" = "/FO" ] && : > "$2"; shift; done
RC
chmod +x target/fake-rc/llvm-rc
RC="$PWD/target/fake-rc/llvm-rc" cargo check --all-targets --target x86_64-pc-windows-msvc

F="--target=x86_64-linux-gnu -isystem $(xcrun --show-sdk-path)/usr/include"
RUST_FONTCONFIG_DLOPEN=1 FREETYPE2_NO_PKG_CONFIG=1 \
  CC_x86_64_unknown_linux_gnu=clang CXX_x86_64_unknown_linux_gnu=clang++ \
  CFLAGS_x86_64_unknown_linux_gnu="$F" CXXFLAGS_x86_64_unknown_linux_gnu="$F" \
  AR_x86_64_unknown_linux_gnu=ar \
  cargo check --all-targets --target x86_64-unknown-linux-gnu
