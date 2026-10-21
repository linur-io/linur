#!/bin/sh
# linur machine gate (D9): build every target, boot QEMU, assert the verdict and
# write verify/report.{json,txt}. Exit 0 only when the expectation is met; with
# --negative the expectation inverts, so a passing gate there is a bug.
set -u

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root" || exit 2

mode=suite
case "${1:-}" in
    --negative) mode=negative ;;
    "") ;;
    *) echo "usage: $0 [--negative]" >&2; exit 2 ;;
esac

if [ "$mode" = negative ]; then
    dir=verify/negative
    test_app=priv_inst
else
    dir=verify
    test_app=1
fi
expected_tests=51
rm -rf "$dir"
mkdir -p "$dir"

commit=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
dirty=false
if [ -n "$(git status --porcelain --untracked-files=no 2>/dev/null)" ]; then
    dirty=true
fi

build_user=fail
build_kernel=fail
host_tests=fail
if make -C user build >"$dir/build-user.log" 2>&1; then
    build_user=pass
fi
if make -C os kernel >"$dir/build-kernel.log" 2>&1; then
    build_kernel=pass
fi
if (cd easy-fs-fuse && cargo test) >"$dir/host-tests.log" 2>&1; then
    host_tests=pass
fi

boot_exit=missing
if [ "$build_user" = pass ] && [ "$build_kernel" = pass ] && [ "$host_tests" = pass ]; then
    make -C os test TEST="$test_app" SERIAL_LOG="$root/$dir/serial.log" >"$dir/boot.log" 2>&1
    if [ -f "$dir/serial.log.status" ]; then
        boot_exit=$(cat "$dir/serial.log.status")
    fi
fi

log="$dir/serial.log"
reported=0
failed=0
if [ -f "$log" ]; then
    reported=$(grep -ac '^RESULT ' "$log" || true)
    failed=$(grep -a '^RESULT ' "$log" | awk '$NF == "FAIL"' | wc -l | tr -d ' ')
fi
passed=$((reported - failed))

verdict=fail
reason=""
if [ "$build_user" != pass ] || [ "$build_kernel" != pass ] || [ "$host_tests" != pass ]; then
    reason="build or host test failed"
elif [ "$boot_exit" = missing ]; then
    reason="no boot status (QEMU did not run)"
elif [ "$boot_exit" = 124 ]; then
    reason="boot timed out"
elif [ "$boot_exit" != 0 ]; then
    reason="boot exited $boot_exit"
elif [ "$reported" -ne "$expected_tests" ]; then
    reason="$reported per-test results, expected $expected_tests"
elif [ "$failed" -ne 0 ]; then
    reason="$failed test(s) failed"
elif ! grep -aq 'Usertests passed!' "$log"; then
    reason="suite did not report Usertests passed"
else
    verdict=pass
fi

if [ "$mode" = negative ]; then
    if [ "$verdict" = pass ]; then
        verdict=unexpected-pass
        reason="a failing boot was expected, but the gate passed"
    else
        verdict=fail-as-expected
    fi
fi

items=$(grep -a '^RESULT ' "$log" 2>/dev/null | awk '{
    expected = ""; actual = "";
    for (i = 3; i <= NF; i++) {
        if ($i ~ /^expected=/) expected = substr($i, 10);
        if ($i ~ /^actual=/) actual = substr($i, 8);
    }
    printf "%s\n    {\"name\": \"%s\", \"expected\": %s, \"actual\": %s, \"verdict\": \"%s\"}", (n++ ? "," : ""), $2, expected, actual, $NF;
}')

timed_out=false
if [ "$boot_exit" = 124 ]; then
    timed_out=true
fi

cat >"$dir/report.json" <<EOF
{
  "schema": 1,
  "mode": "$mode",
  "commit": "$commit",
  "dirty": $dirty,
  "started_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "target": "riscv64-qemu",
  "build": {
    "user": "$build_user",
    "kernel": "$build_kernel",
    "host_tests": "$host_tests"
  },
  "boot": {
    "exit_code": "$boot_exit",
    "expected": "$([ "$mode" = negative ] && echo nonzero || echo 0)",
    "timed_out": $timed_out
  },
  "tests": {
    "expected": $expected_tests,
    "reported": $reported,
    "passed": $passed,
    "failed": $failed,
    "items": [
$items
    ]
  },
  "verdict": "$verdict",
  "reason": "$reason"
}
EOF

{
    echo "linur verify — $mode — $verdict"
    echo "commit $commit (dirty=$dirty)"
    echo "build: user $build_user, kernel $build_kernel, host tests $host_tests"
    echo "boot: exit $boot_exit, timeout $timed_out"
    echo "tests: $passed/$reported passed, $failed failed (expected $expected_tests)"
    if [ -n "$reason" ]; then
        echo "reason: $reason"
    fi
    if [ "$failed" -gt 0 ]; then
        echo "failed tests:"
        grep -a '^RESULT ' "$log" | awk '$NF == "FAIL" { print "  " $2 }'
    fi
} >"$dir/report.txt"

cat "$dir/report.txt"

case "$verdict" in
    pass|fail-as-expected) exit 0 ;;
    *) exit 1 ;;
esac
