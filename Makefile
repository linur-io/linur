# linur machine gate (D9): build every target, boot QEMU, assert the verdict,
# and write verify/report.{json,txt}. The work lives in scripts/verify.sh.
verify:
	@sh scripts/verify.sh

# The gate must fail on a known-bad boot; exits 0 when it observed the failure.
verify-negative:
	@sh scripts/verify.sh --negative

# Static checks. clippy is a hard gate for the kernel crates; the 55 user
# programs are fixtures until the own libc lands (M3.3), so theirs is advisory.
lint:
	cd os && cargo fmt --check && cargo clippy -- -D warnings
	cd easy-fs && cargo fmt --check && cargo clippy -- -D warnings
	cd user && cargo fmt --check && cargo clippy

.PHONY: verify verify-negative lint
