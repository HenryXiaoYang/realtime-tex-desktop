#!/bin/sh
# A test build to hand out: "Realtime Tex Desktop.app" for Apple-chip Macs with the rtex engine
# inside, ad-hoc signed, in target/release/Realtime-Tex-Desktop-<version>-arm64.dmg.
# The engine comes from $RTEX_RELEASE (an unpacked release folder), else the latest rtex release,
# downloaded. Recipients still need a TeX distribution (MacTeX) for lualatex.
set -e
cd "$(dirname "$0")/.."
./scripts/bundle.sh >/dev/null
app="target/release/Realtime Tex Desktop.app"
if [ -z "$RTEX_RELEASE" ]; then
  dl=target/rtex-release
  rm -rf "$dl" && mkdir -p "$dl"
  curl -fsSL https://github.com/HenryXiaoYang/realtime-tex/releases/latest/download/rtex-aarch64-apple-darwin.tar.gz \
    | tar -xzf - -C "$dl"
  RTEX_RELEASE=$(ls -d "$dl"/rtex-* | tail -1)
fi
rtex="$RTEX_RELEASE"
[ -x "$rtex/bin/rtex" ] || { echo "no rtex release found (set RTEX_RELEASE)" >&2; exit 1; }
res="$app/Contents/Resources/rtex"
mkdir -p "$res"
cp -R "$rtex/bin" "$rtex/share" "$rtex/LICENSE" "$res/"
# unsigned code is refused on Apple silicon; an ad-hoc signature is enough to run
codesign --force --deep --sign - "$app"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
dmg="target/release/Realtime-Tex-Desktop-$version-arm64.dmg"
stage=$(mktemp -d)
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
cat > "$stage/Read Me.txt" <<TXT
Realtime Tex Desktop $version (test build, Apple-chip Macs)

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
