#!/bin/sh
set -eu
[ "$1" = -q ] || exit 2
step=download
output=
proto=
redir=
url=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --write-out) step=view; shift 2 ;;
        --output) output=$2; shift 2 ;;
        --proto) proto=$2; shift 2 ;;
        --proto-redir) redir=$2; shift 2 ;;
        --retry) shift 2 ;;
        -*) shift ;;
        *) url=$1; shift ;;
    esac
done
[ "$proto" = '=https' ] && [ "$redir" = '=https' ] || exit 2
[ "$step" != "$MOCK_FAIL" ] || exit 1
base=https://github.com/zcaceres/easy-tix/releases/
if [ "$step" = view ]; then
    [ "$url" = "${base}latest" ] || exit 2
    printf '%stag/%s\n' "$base" "$MOCK_TAG"
else
    case "$url" in "${base}download/$MOCK_TAG/"*) ;; *) exit 2 ;; esac
    cp "$MOCK_DOWNLOADS/${url##*/}" "$output"
fi
