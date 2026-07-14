#!/bin/sh
# tq installer — https://github.com/reddb-io/toon
#
#   curl -fsSL https://raw.githubusercontent.com/reddb-io/toon/main/install.sh | sh
#
# Detects OS/architecture, resolves the latest release, and installs the
# right prebuilt binary. If tq is already installed it becomes an update:
# same version → no-op, different version → replaced in place.
#
# Environment knobs:
#   TQ_INSTALL_DIR  target directory (default: alongside an existing tq,
#                   else /usr/local/bin when writable, else ~/.local/bin)
#   TQ_VERSION      pin a tag, e.g. v0.1.0 (default: latest release)
#   TQ_CHANNEL      "stable" (default) or "next" — next allows prereleases
#   TQ_FORCE        set to 1 to reinstall even when already up to date
#   GITHUB_TOKEN    used when set (required while the repo is private)

set -u

REPO="reddb-io/toon"
API="https://api.github.com/repos/${REPO}"

say() { printf '%s\n' "$*"; }
err() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

have() { command -v "$1" >/dev/null 2>&1; }

# --- fetch helpers (curl preferred, wget fallback) ---------------------------

fetch() { # fetch <url> [output-file]; stdout when no file given
  _url="$1"; _out="${2:-}"
  if have curl; then
    if [ -n "${GITHUB_TOKEN:-}" ]; then
      set -- -fsSL -H "Authorization: Bearer ${GITHUB_TOKEN}" "$_url"
    else
      set -- -fsSL "$_url"
    fi
    if [ -n "$_out" ]; then curl "$@" -o "$_out"; else curl "$@"; fi
  elif have wget; then
    if [ -n "${GITHUB_TOKEN:-}" ]; then
      set -- -q --header="Authorization: Bearer ${GITHUB_TOKEN}" "$_url"
    else
      set -- -q "$_url"
    fi
    if [ -n "$_out" ]; then wget "$@" -O "$_out"; else wget "$@" -O -; fi
  else
    err "need curl or wget"
  fi
}

fetch_asset() { # fetch_asset <asset-name> <release-json-file> <output-file>
  _name="$1"; _json="$2"; _dest="$3"
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    # Private repos serve assets only through the API asset endpoint. In the
    # release JSON each asset's ".../releases/assets/<id>" url precedes its
    # "name", so remember the last id seen and emit it on the name match.
    _id="$(tr ',' '\n' <"$_json" | awk -v name="$_name" '
      match($0, /releases\/assets\/[0-9]+/) {
        id = substr($0, RSTART + 16, RLENGTH - 16)
      }
      $0 ~ "\"name\":[[:space:]]*\"" name "\"" { print id; exit }
    ')"
    [ -n "$_id" ] || err "asset ${_name} not found in release ${TAG}"
    if have curl; then
      curl -fsSL -H "Authorization: Bearer ${GITHUB_TOKEN}" \
        -H "Accept: application/octet-stream" \
        "${API}/releases/assets/${_id}" -o "$_dest"
    else
      wget -q --header="Authorization: Bearer ${GITHUB_TOKEN}" \
        --header="Accept: application/octet-stream" \
        "${API}/releases/assets/${_id}" -O "$_dest"
    fi
  else
    fetch "https://github.com/${REPO}/releases/download/${TAG}/${_name}" "$_dest"
  fi
}

# --- resolve platform --------------------------------------------------------

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64 | amd64) ASSET="tq-linux-x86_64-static" ;;
      aarch64 | arm64) ASSET="tq-linux-aarch64-static" ;;
      *) err "unsupported Linux architecture: ${ARCH}" ;;
    esac
    ;;
  Darwin)
    case "$ARCH" in
      x86_64) ASSET="tq-macos-x86_64" ;;
      arm64) ASSET="tq-macos-aarch64" ;;
      *) err "unsupported macOS architecture: ${ARCH}" ;;
    esac
    ;;
  *)
    err "unsupported OS: ${OS} (on Windows, grab tq-windows-x86_64.exe from https://github.com/${REPO}/releases)"
    ;;
esac

# --- resolve release ---------------------------------------------------------

TMP="$(mktemp -d "${TMPDIR:-/tmp}/tq-install.XXXXXX")" || err "mktemp failed"
trap 'rm -rf "$TMP"' EXIT INT TERM

RELEASE_JSON="${TMP}/release.json"
if [ -n "${TQ_VERSION:-}" ]; then
  fetch "${API}/releases/tags/${TQ_VERSION}" >"$RELEASE_JSON" \
    || err "release ${TQ_VERSION} not found"
elif [ "${TQ_CHANNEL:-stable}" = "next" ]; then
  fetch "${API}/releases?per_page=1" | sed 's/^\[//;s/\]$//' >"$RELEASE_JSON" \
    || err "could not list releases"
else
  # Latest stable; while only prereleases exist, fall back to the newest one.
  fetch "${API}/releases/latest" >"$RELEASE_JSON" 2>/dev/null \
    || fetch "${API}/releases?per_page=1" | sed 's/^\[//;s/\]$//' >"$RELEASE_JSON" \
    || err "could not resolve the latest release"
fi

TAG="$(tr ',' '\n' <"$RELEASE_JSON" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
[ -n "$TAG" ] || err "could not parse the release tag (private repo without GITHUB_TOKEN?)"
VERSION="${TAG#v}"

# --- already installed? ------------------------------------------------------

EXISTING="$(command -v tq 2>/dev/null || true)"
if [ -n "$EXISTING" ]; then
  CURRENT="$("$EXISTING" --version 2>/dev/null | sed -n 's/^tq //p' || true)"
  if [ "$CURRENT" = "$VERSION" ] && [ "${TQ_FORCE:-0}" != "1" ]; then
    say "tq ${VERSION} is already installed at ${EXISTING} — nothing to do."
    exit 0
  fi
fi

# --- pick the install dir ----------------------------------------------------

if [ -n "${TQ_INSTALL_DIR:-}" ]; then
  DIR="$TQ_INSTALL_DIR"
elif [ -n "$EXISTING" ] && [ -w "$(dirname "$EXISTING")" ]; then
  DIR="$(dirname "$EXISTING")"
elif [ -d /usr/local/bin ] && [ -w /usr/local/bin ]; then
  DIR="/usr/local/bin"
else
  DIR="${HOME}/.local/bin"
fi
mkdir -p "$DIR" || err "cannot create ${DIR}"
[ -w "$DIR" ] || err "no write permission for ${DIR} (set TQ_INSTALL_DIR or re-run with sudo)"

# --- download + verify -------------------------------------------------------

say "Downloading ${ASSET} ${TAG}..."
fetch_asset "$ASSET" "$RELEASE_JSON" "${TMP}/tq"
fetch_asset "SHA256SUMS" "$RELEASE_JSON" "${TMP}/SHA256SUMS"

(
  cd "$TMP"
  grep "  ${ASSET}\$" SHA256SUMS | sed "s|  ${ASSET}\$|  tq|" >expected.sha256
  [ -s expected.sha256 ] || err "no checksum for ${ASSET} in SHA256SUMS"
  if have sha256sum; then
    sha256sum -c expected.sha256 >/dev/null
  elif have shasum; then
    shasum -a 256 -c expected.sha256 >/dev/null
  else
    err "need sha256sum or shasum to verify the download"
  fi
) || err "checksum verification failed for ${ASSET}"

chmod 755 "${TMP}/tq"
"${TMP}/tq" --version >/dev/null 2>&1 || err "downloaded binary does not run on this machine"

# --- install -----------------------------------------------------------------

mv "${TMP}/tq" "${DIR}/tq" || err "could not move tq into ${DIR}"

if [ -n "$EXISTING" ] && [ "$EXISTING" != "${DIR}/tq" ]; then
  say "note: another tq remains at ${EXISTING}; PATH order decides which one runs."
fi

if [ -n "${CURRENT:-}" ]; then
  say "Updated tq ${CURRENT} -> ${VERSION} at ${DIR}/tq"
else
  say "Installed tq ${VERSION} at ${DIR}/tq"
fi

case ":${PATH}:" in
  *":${DIR}:"*) ;;
  *) say "note: ${DIR} is not on your PATH — add it, e.g.: export PATH=\"${DIR}:\$PATH\"" ;;
esac

"${DIR}/tq" --version
