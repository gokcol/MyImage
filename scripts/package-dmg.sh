#!/bin/sh
# Wrap an already signed, stapled release app without rebuilding or changing it.
# MYIMAGE_SIGNING_IDENTITY=<fingerprint> MYIMAGE_NOTARY_PROFILE=MyPDF \
#   sh scripts/package-dmg.sh target/bundle/MyImage.app dist/installers
set -eu
app=${1:?Supply the signed, notarized MyImage.app}
out=${2:?Supply an output directory}
identity=${MYIMAGE_SIGNING_IDENTITY:?Set the Developer ID Application fingerprint}
profile=${MYIMAGE_NOTARY_PROFILE:?Set an existing notarytool Keychain profile}
team=HXDYTBC6ZT
test -d "$app" && test ! -L "$app"
app=$(cd "$(dirname "$app")" && pwd)/$(basename "$app")
plist=$app/Contents/Info.plist
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")" = com.myimage.app
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")
build=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$plist")
case "$version.$build" in *[!0-9.]*|'') echo 'Invalid app version' >&2; exit 1;; esac
arch=$(lipo -archs "$app/Contents/MacOS/MyImage")
case "$arch" in arm64|x86_64) ;; *) echo "Unsupported DMG architecture: $arch" >&2; exit 1;; esac
name=MyImage-$version-build$build-macos-$arch.dmg
mkdir -p "$out"
out=$(cd "$out" && pwd)
test ! -e "$out/$name" || { echo "Output already exists: $out/$name" >&2; exit 1; }
codesign --verify --deep --strict -R "=anchor apple generic and certificate leaf[subject.OU] = \"$team\" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists" "$app"
xcrun stapler validate "$app"
spctl --assess --type execute --verbose=2 "$app"
work=$(mktemp -d "$out/.dmg-work.XXXXXX")
mounted=false
cleanup() {
    status=$?
    trap - EXIT HUP INT TERM
    if [ "$mounted" = true ]; then hdiutil detach "$work/mount" || exit 1; fi
    rm -rf "$work"
    exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir "$work/layout"
ditto "$app" "$work/layout/MyImage.app"
ln -s /Applications "$work/layout/Applications"
cat > "$work/layout/Install MyImage.txt" <<'TEXT'
Drag MyImage.app onto Applications, then open MyImage from Applications.
If updating, save your work and quit MyImage from its menu before replacing it.
Your projects, captures, settings and recovery data are kept separately.
Requires macOS 14 or later. Source: https://github.com/gokcol/MyImage
TEXT
hdiutil create -volname "MyImage $version" -srcfolder "$work/layout" -format UDZO "$work/$name"
codesign --force --timestamp --sign "$identity" "$work/$name"
codesign --verify --strict -R "=anchor apple generic and certificate leaf[subject.OU] = \"$team\"" "$work/$name"
xcrun notarytool submit "$work/$name" --keychain-profile "$profile" --wait --output-format json > "$work/notary.json"
cat "$work/notary.json"
python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); sys.exit(0 if r.get("status")=="Accepted" else "DMG notarization was not accepted")' "$work/notary.json"
xcrun stapler staple "$work/$name"
xcrun stapler validate "$work/$name"
spctl --assess --type open --context context:primary-signature --verbose=2 "$work/$name"
hdiutil verify "$work/$name"
mkdir "$work/mount"
hdiutil attach -readonly -nobrowse -mountpoint "$work/mount" "$work/$name"
mounted=true
test "$(readlink "$work/mount/Applications")" = /Applications
cmp "$app/Contents/MacOS/MyImage" "$work/mount/MyImage.app/Contents/MacOS/MyImage"
codesign --verify --deep --strict "$work/mount/MyImage.app"
xcrun stapler validate "$work/mount/MyImage.app"
hdiutil detach "$work/mount"
mounted=false
(cd "$work" && shasum -a 256 "$name" > "$name.sha256")
mv "$work/$name" "$out/$name"
mv "$work/$name.sha256" "$out/$name.sha256"
mv "$work/notary.json" "$out/$name.notary.json"
echo "Signed, notarized and verified: $out/$name"
