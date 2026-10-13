#!/usr/bin/env bash
# Repack the Linux AppImage without its over-bundled Wayland libraries, RE-SIGN it, re-upload it,
# and point latest.json at the new signature.
#
# Runs on the Linux release leg AFTER tauri-action has built, signed and uploaded. Each step
# exists because skipping it fails silently rather than loudly:
#
#   repack      without it the AppImage opens a blank window on Mesa 25+ (tauri#15665); see
#               strip-appimage-wayland.sh for the measurement
#   re-sign     the Tauri updater's Linux artifact IS the AppImage, and its .sig covers the
#               pre-repack bytes. A stale signature still LOOKS signed: the app installs, and the
#               first update check rejects every download forever
#   re-upload   the draft would otherwise carry the old AppImage and the old .sig
#   manifest    latest.json embeds the signature text itself, not a link to the .sig file, so
#               uploading a new .sig alone changes nothing a client reads
#
# Usage: resign-linux-appimage.sh <bundle-dir> <tag> <appimagetool>
# Env:   GH_TOKEN, GITHUB_REPOSITORY, TAURI_SIGNING_PRIVATE_KEY, TAURI_SIGNING_PRIVATE_KEY_PASSWORD
set -euo pipefail

bundle_dir="${1:?usage: $0 <bundle-dir> <tag> <appimagetool>}"
tag="${2:?usage: $0 <bundle-dir> <tag> <appimagetool>}"
appimagetool="${3:?usage: $0 <bundle-dir> <tag> <appimagetool>}"
here="$(cd "$(dirname "$0")" && pwd)"
: "${GITHUB_REPOSITORY:?}" "${TAURI_SIGNING_PRIVATE_KEY:?}"

mapfile -t images < <(find "$bundle_dir/appimage" -maxdepth 1 -name '*.AppImage' | sort)
if [ "${#images[@]}" -ne 1 ]; then
  echo "::error::expected exactly one AppImage in $bundle_dir/appimage, found ${#images[@]}"
  exit 1
fi
image="${images[0]}"
name="$(basename "$image")"

bash "$here/strip-appimage-wayland.sh" "$image" "$appimagetool"

rm -f "$image.sig"
npx --no-install tauri signer sign "$image"
[ -s "$image.sig" ] || { echo "::error::tauri signer produced no $name.sig"; exit 1; }

# GitHub stores "Redrob Recall_x.AppImage" as "Redrob.Recall_x.AppImage": the upload keeps the
# bytes and dots the space. The listing and the manifest both use that stored name.
gh_name="${name// /.}"

# The release is a DRAFT, which the tags endpoint does not resolve, so find it in the listing.
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
gh api "repos/${GITHUB_REPOSITORY}/releases?per_page=100" > "$work/releases.json"
release_json="$(jq --arg tag "$tag" 'map(select(.tag_name == $tag)) | first // empty' "$work/releases.json")"
manifest_id="$(printf '%s' "$release_json" | jq -r '.assets[] | select(.name == "latest.json") | .id')"
if [ -z "$manifest_id" ] || [ "$manifest_id" = "null" ]; then
  echo "::error::latest.json is not attached to $tag yet; the manifest cannot be updated"
  exit 1
fi
# On a draft, tauri-action writes each platform URL as the REST asset endpoint
# (.../releases/assets/<id>), not as a URL that ends in the file name -- a draft has no public
# download URL yet. `--clobber` below deletes that asset and uploads a new one under a NEW id, so
# the id the manifest names is read now, before it is gone; matching on the name alone finds
# nothing, and this script would then fail after the AppImage had already been replaced.
old_id="$(printf '%s' "$release_json" | jq -r --arg n "$gh_name" '.assets[] | select(.name == $n) | .id')"
if [ -z "$old_id" ] || [ "$old_id" = "null" ]; then
  echo "::error::$gh_name is not attached to $tag; tauri-action did not upload the AppImage"
  exit 1
fi
gh api -H 'Accept: application/octet-stream' \
  "repos/${GITHUB_REPOSITORY}/releases/assets/${manifest_id}" > "$work/latest.json"

gh release upload "$tag" "$image" "$image.sig" --clobber --repo "$GITHUB_REPOSITORY"

# Replace the signature on every platform entry that is this AppImage -- tauri-action writes both
# `linux-x86_64` and `linux-x86_64-appimage`, and missing one leaves a client that resolves the
# other key verifying against stale bytes. The URL is written in its published form at the same
# time: the REST id it carried is dangling now, and the published URL is fully determined by the
# tag and the name (the verify job would write exactly this; rename-release-assets.mjs repoints it
# again when it renames the file).
sig="$(cat "$image.sig")"
published="https://github.com/${GITHUB_REPOSITORY}/releases/download/${tag}/${gh_name}"
jq --arg name "$gh_name" --arg id "$old_id" --arg sig "$sig" --arg url "$published" '
  .platforms |= with_entries(
    if ((.value.url | endswith("/" + $name)) or (.value.url | endswith("/releases/assets/" + $id)))
    then .value.signature = $sig | .value.url = $url else . end)
' "$work/latest.json" > "$work/latest.new.json"

replaced="$(jq --arg url "$published" --arg sig "$sig" \
  '[.platforms[] | select(.url == $url and .signature == $sig)] | length' \
  "$work/latest.new.json")"
if [ "$replaced" -lt 1 ]; then
  echo "::error::no latest.json platform entry points at $gh_name; nothing was re-signed in the manifest"
  exit 1
fi

cp "$work/latest.new.json" "$work/latest.json"
gh release upload "$tag" "$work/latest.json" --clobber --repo "$GITHUB_REPOSITORY"
echo "re-signed $gh_name and updated $replaced latest.json platform entr$( [ "$replaced" -eq 1 ] && echo y || echo ies)"
