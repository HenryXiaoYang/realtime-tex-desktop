#!/bin/sh
# A test build to hand out: "Realtime Tex Desktop.app" for this Mac's kind of chip with the rtex
# engine inside, ad-hoc signed, in target/release/Realtime-Tex-Desktop-<version>-<target>.dmg.
# The engine comes from $RTEX_RELEASE (an unpacked release folder), else the rtex release the app
# pins (src/engine/download.rs: its tag and checksum), downloaded. Recipients still need a TeX
# distribution (MacTeX) for lualatex.
set -e
cd "$(dirname "$0")/.."
./scripts/bundle.sh >/dev/null
app="target/release/Realtime Tex Desktop.app"
case $(uname -m) in
  arm64) target=aarch64-apple-darwin chip="Apple-chip Macs" ;;
  *) target=x86_64-apple-darwin chip="Intel Macs" ;;
esac
if [ -z "$RTEX_RELEASE" ]; then
  dl=target/rtex-release
  rm -rf "$dl" && mkdir -p "$dl"
  asset=rtex-$target.tar.gz
  tag=$(sed -n 's/^const RTEX_TAG: &str = "\(.*\)";/\1/p' src/engine/download.rs)
  sum=$(grep -A1 "\"$asset\"" src/engine/download.rs | sed -n 's/.*"\([0-9a-f]\{64\}\)".*/\1/p')
  curl -fsSL --proto =https --proto-redir =https -o "$dl/$asset" \
    "https://github.com/HenryXiaoYang/realtime-tex/releases/download/$tag/$asset"
  echo "$sum  $dl/$asset" | shasum -a 256 -c - >/dev/null
  tar -xzf "$dl/$asset" -C "$dl"
  RTEX_RELEASE=$(ls -d "$dl"/rtex-*/ | tail -1)
fi
rtex="$RTEX_RELEASE"
[ -x "$rtex/bin/rtex" ] || { echo "no rtex release found (set RTEX_RELEASE)" >&2; exit 1; }
res="$app/Contents/Resources/rtex"
mkdir -p "$res"
cp -R "$rtex/bin" "$rtex/share" "$rtex/LICENSE" "$res/"
# unsigned code is refused on Apple silicon; an ad-hoc signature is enough to run
codesign --force --deep --sign - "$app"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
dmg="target/release/Realtime-Tex-Desktop-$version-$target.dmg"
stage=$(mktemp -d)
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
cat > "$stage/Read Me.txt" <<TXT
Realtime Tex Desktop $version (test build, $chip)

1. Drag Realtime Tex Desktop into Applications.
2. Install MacTeX (https://tug.org/mactex/) if you do not have it: the app needs lualatex.
3. The app is not notarized. The first time, open it, then go to System Settings > Privacy &
   Security and click "Open Anyway" (or run: xattr -dr com.apple.quarantine
   "/Applications/Realtime Tex Desktop.app").
4. Open a folder with your .tex project when the app asks.
TXT
rm -f "$dmg"
hdiutil create -volname "Realtime Tex Desktop" -srcfolder "$stage" -ov -format UDZO "$dmg" >/dev/null
rm -rf "$stage"
echo "$dmg"
