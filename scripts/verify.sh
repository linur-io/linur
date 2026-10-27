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
expected_tests=77
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
if { (cd fs && cargo test --workspace) && (cd elf && cargo test) && (cd heap && cargo test); } >"$dir/host-tests.log" 2>&1; then
    host_tests=pass
fi

boot_exit=missing
if [ "$build_user" = pass ] && [ "$build_kernel" = pass ] && [ "$host_tests" = pass ]; then
    make -C os test TEST="$test_app" SERIAL_LOG="$root/$dir/serial.log" >"$dir/boot.log" 2>&1
    if [ -f "$dir/serial.log.status" ]; then
        boot_exit=$(cat "$dir/serial.log.status")
    fi
fi

# A lone task blocked in sleep must be woken by the idle loop. The suite cannot
# show this -- its parent is always runnable -- so one single-app boot guards the
# wakeup path. A pass is QEMU status 0 plus the program's final line.
solo_exit=skipped
solo_marker=skipped
if [ "$mode" = suite ]; then
    solo_exit=missing
    solo_marker=fail
    if [ "$build_user" = pass ] && [ "$build_kernel" = pass ]; then
        make -C os test TEST=sleep_simple TEST_TIMEOUT=30 SERIAL_LOG="$root/$dir/solo-serial.log" >"$dir/solo-boot.log" 2>&1
        if [ -f "$dir/solo-serial.log.status" ]; then
            solo_exit=$(cat "$dir/solo-serial.log.status")
        fi
        if [ -f "$dir/solo-serial.log" ] && grep -aqF 'r_sleep passed!' "$dir/solo-serial.log"; then
            solo_marker=pass
        fi
    fi
fi

# M1.5 x86_64 smoke path (docs/x86_64-smoke.md): compile-only check, image build,
# then boot. A pass is raw QEMU status 33 plus both serial markers. --negative
# exercises the riscv64 bad boot only, so the x86 phases report skipped there.
x86_check=skipped
x86_build=skipped
x86_boot_exit=skipped
x86_serial=skipped
if [ "$mode" = suite ]; then
    x86_check=fail
    x86_build=fail
    x86_boot_exit=missing
    x86_serial=fail
    if (cd x86-smoke && cargo check --target x86_64-unknown-none) >"$dir/x86-check.log" 2>&1; then
        x86_check=pass
    fi
    if make -C x86-smoke image >"$dir/x86-build.log" 2>&1; then
        x86_build=pass
    fi
    # Booting a stale or missing image would be a misleading pass, so the boot
    # phase only runs after both build phases succeeded.
    if [ "$x86_check" = pass ] && [ "$x86_build" = pass ]; then
        make -C x86-smoke test SERIAL_LOG="$root/$dir/x86-serial.log" >"$dir/x86-boot.log" 2>&1
        if [ -f "$dir/x86-serial.log.status" ]; then
            x86_boot_exit=$(cat "$dir/x86-serial.log.status")
        fi
        if [ -f "$dir/x86-serial.log" ] && [ "$x86_boot_exit" = 33 ] &&
            grep -aqF '[x86_64] long mode' "$dir/x86-serial.log" &&
            grep -aqF '[x86_64] serial ok' "$dir/x86-serial.log"; then
            x86_serial=pass
        fi
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

# Any failed x86 phase fails the suite mode verdict; the riscv64 reason wins when
# both halves fail, so the first failure stays the reported one.
if [ "$mode" = suite ]; then
    x86_reason=""
    if [ "$x86_check" != pass ]; then
        x86_reason="x86_64 compile-only check failed"
    elif [ "$x86_build" != pass ]; then
        x86_reason="x86_64 image build failed"
    elif [ "$x86_boot_exit" != 33 ]; then
        x86_reason="x86_64 boot exited $x86_boot_exit"
    elif [ "$x86_serial" != pass ]; then
        x86_reason="x86_64 serial markers missing"
    fi
    if [ -n "$x86_reason" ]; then
        verdict=fail
        if [ -z "$reason" ]; then
            reason="$x86_reason"
        fi
    fi
    solo_reason=""
    if [ "$solo_exit" != 0 ]; then
        solo_reason="lone sleeper boot exited $solo_exit"
    elif [ "$solo_marker" != pass ]; then
        solo_reason="lone sleeper did not report r_sleep passed"
    fi
    if [ -n "$solo_reason" ]; then
        verdict=fail
        if [ -z "$reason" ]; then
            reason="$solo_reason"
        fi
    fi
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
  "x86_64": {
    "check": "$x86_check",
    "build": "$x86_build",
    "boot": {
      "exit_code": "$x86_boot_exit",
      "expected": "33",
      "serial": "$x86_serial"
    }
  },
  "solo_sleep": {
    "app": "sleep_simple",
    "boot": {
      "exit_code": "$solo_exit",
      "expected": "0",
      "marker": "$solo_marker"
    }
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
    echo "x86_64: check $x86_check, image $x86_build, boot exit $x86_boot_exit, serial $x86_serial (expected 33)"
    echo "solo: sleep_simple exit $solo_exit, marker $solo_marker (expected 0)"
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
