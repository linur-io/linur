#!/bin/sh
# Resolve the ra=0x... frames of a kernel backtrace block to symbol:file:line.
# Usage: symbolize.sh <log> [kernel-elf]

set -eu

LOG=${1:-}
[ -n "$LOG" ] || { echo "usage: symbolize.sh <log> [kernel-elf]" >&2; exit 1; }
[ -f "$LOG" ] || { echo "error: no such log: $LOG" >&2; exit 1; }

ELF=${2:-$(cd "$(dirname "$0")/.." && pwd)/target/riscv64gc-unknown-none-elf/release/os}
[ -f "$ELF" ] || { echo "error: no such kernel image: $ELF (run make first)" >&2; exit 1; }

# llvm-tools does not ship llvm-addr2line; binutils' addr2line decodes the ELF.
command -v addr2line >/dev/null 2>&1 || {
	echo "error: addr2line not found (install binutils)" >&2
	exit 1
}

frames=$(awk '/^---START BACKTRACE---$/{f=1; next} /^---END   BACKTRACE---$/{f=0} f' "$LOG" |
	sed -n 's/^#\([0-9][0-9]*\):[[:space:]]*ra=0x\([0-9a-fA-F]*\).*/\1 \2/p')
[ -n "$frames" ] || {
	echo "error: no '#<n>: ra=0x...' frame between the backtrace markers in $LOG" >&2
	exit 1
}

resolved=0
while read -r n hex; do
	# ra points at the instruction after the call, so resolve ra-1.
	line=$(addr2line -f -C -e "$ELF" "$(printf '0x%x' $((0x$hex - 1)))")
	fn=$(printf '%s\n' "$line" | sed -n 1p)
	loc=$(printf '%s\n' "$line" | sed -n 2p)
	[ "$fn" = "??" ] || resolved=$((resolved + 1))
	printf '#%s 0x%s %s\n\t%s\n' "$n" "$hex" "$fn" "$loc"
done <<EOF
$frames
EOF

# Assembly frames have no DWARF and print as ??; only a fully dead log fails.
[ "$resolved" -gt 0 ] || {
	echo "error: no frame in $LOG resolved in $ELF" >&2
	exit 1
}
