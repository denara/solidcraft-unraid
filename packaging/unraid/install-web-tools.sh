#!/bin/sh
# Download or install the exact Linux amd64 tools used by the two Trunk builds.
set -eu

mode=$1
lockfile=$2
assets=$3
wasm_bindgen_version=0.2.129
locked_version=$(awk '
    { sub(/\r$/, "") }
    $0 == "name = \"wasm-bindgen\"" { getline; gsub(/["\r]/, "", $3); print $3 }
' "$lockfile")
if [ "$locked_version" != "$wasm_bindgen_version" ]; then
    printf >&2 'Cargo.lock needs wasm-bindgen %s; update the pinned CLI asset and SHA256.\n' "$locked_version"
    exit 1
fi

bindgen_sha=82d12bb940e2d4e72e0d5605387fc1b8ca179044e012b620f0ce4e7440e8320e
binaryen_sha=e959f2170af4c20c552e9de3a0253704d6a9d2766e8fdb88e4d6ac4bae9388fe

fetch_asset() {
    endpoint="repos/$1/releases/assets/$2?download=1"
    if [ -n "${GH_TOKEN:-}" ]; then
        gh api "$endpoint" --header 'Accept: application/octet-stream' > "$3"
    else
        curl --fail --silent --show-error --location --retry 3 --connect-timeout 10 --max-time 120 \
            --proto '=https' --proto-redir '=https' \
            --header 'Accept: application/octet-stream' \
            --header 'X-GitHub-Api-Version: 2022-11-28' \
            --output "$3" "https://api.github.com/$endpoint"
    fi
}

fetch_tools() {
    mkdir -p "$assets"
    fetch_asset wasm-bindgen/wasm-bindgen 587769529 "$assets/wasm-bindgen.tar.gz"
    fetch_asset WebAssembly/binaryen 240710285 "$assets/binaryen.tar.gz"
}

case "$mode" in
    download) fetch_tools ;;
    install)
        if [ ! -e "$assets/wasm-bindgen.tar.gz" ] && [ ! -e "$assets/binaryen.tar.gz" ]; then
            # Ordinary Docker builds can fetch public assets without any credentials.
            assets=$(mktemp -d)
            trap 'rm -rf -- "$assets"' EXIT
            fetch_tools
        fi
        ;;
    *) printf >&2 'Expected download or install.\n'; exit 1 ;;
esac

printf '%s  %s\n' "$bindgen_sha" "$assets/wasm-bindgen.tar.gz" \
    "$binaryen_sha" "$assets/binaryen.tar.gz" | sha256sum --check -
if [ "$mode" = download ]; then
    exit 0
fi

tar -xzf "$assets/wasm-bindgen.tar.gz" -C /usr/local/bin --strip-components=1 \
    "wasm-bindgen-$wasm_bindgen_version-x86_64-unknown-linux-musl/wasm-bindgen"
tar -xzf "$assets/binaryen.tar.gz" -C /usr/local/bin --strip-components=2 \
    binaryen-version_123/bin/wasm-opt
test "$(wasm-bindgen --version)" = "wasm-bindgen $wasm_bindgen_version"
test "$(wasm-opt --version | awk '{print $1, $2, $3}')" = 'wasm-opt version 123'
