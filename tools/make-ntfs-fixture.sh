#!/usr/bin/env bash
# Build an NTFS volume image and a listing of what is in it, for testing the
# file-table reader in crates/strata-mft. Needs ntfs-3g (mkntfs, FUSE mount).
#
#   tools/make-ntfs-fixture.sh <out-prefix> [size-MB] [extra-files]
#
# Writes <out-prefix>.img and <out-prefix>.expected.tsv. The listing has one
# line per entry: kind (d or f), size in bytes, 512-byte blocks, inode, path.
# Its last line is "used <bytes>": the space in use according to the volume.
set -euo pipefail
out=$1; size_mb=${2:-12}; extra=${3:-0}
img=$out.img; mnt=$(mktemp -d)
rm -f "$img"; truncate -s "${size_mb}M" "$img"
mkntfs -F -q -L STRATA "$img" >/dev/null 2>&1
ntfs-3g "$img" "$mnt"
trap 'fusermount -u "$mnt" 2>/dev/null || true; rmdir "$mnt" 2>/dev/null || true' EXIT
cd "$mnt"

fill() { head -c "$2" /dev/zero | tr '\0' "${3:-x}" > "$1"; }

mkdir -p docs "Ünïcödé 日本語" deep/a/b/c/d/e/f/g/h big sparse hl other many emptydir node_modules/pkg "Program Files/Tool"
fill docs/report.pdf 300000
printf 'twenty bytes of text' > docs/tiny.txt
: > docs/empty.txt
fill "Ünïcödé 日本語/файл.txt" 5000
fill deep/a/b/c/d/e/f/g/h/bottom.bin 5000
fill big/movie.mp4 2500000 m
truncate -s 6M sparse/hole.img
printf 'tail' >> sparse/hole.img
fill hl/original.dat 200000 h
ln hl/original.dat other/second-name.dat
ln hl/original.dat hl/third-name.dat
for i in $(seq -w 1 1500); do printf 'n%s' "$i" > "many/f$i.txt"; done
fill node_modules/pkg/index.js 70000 j
fill "Program Files/Tool/tool.exe" 90000 e
fill "A long file name that would need a short alias.document" 40000
if [ "$extra" -gt 0 ]; then
  for d in $(seq 1 $((extra / 500 + 1))); do
    mkdir -p "bulk/d$d"
    for i in $(seq 1 500); do printf '%s' "$i" > "bulk/d$d/f$i"; done
  done
fi
sync
cd - >/dev/null
( cd "$mnt" && find . -mindepth 1 \( -type d -printf 'd\t0\t%b\t%i\t%P\n' \) -o \( -type f -printf 'f\t%s\t%b\t%i\t%P\n' \) | LC_ALL=C sort -t$'\t' -k5 ) > "$out.expected.tsv"
echo -e "used\t$(df -B1 --output=used "$mnt" | tail -1 | tr -d ' ')" >> "$out.expected.tsv"
fusermount -u "$mnt"; rmdir "$mnt"; trap - EXIT
echo "wrote $img and $out.expected.tsv"
