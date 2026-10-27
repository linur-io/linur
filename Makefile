# linur machine gate (D9): build every target, boot QEMU, assert the verdict,
# and write verify/report.{json,txt}. The work lives in scripts/verify.sh.
verify:
	@sh scripts/verify.sh

# The gate must fail on a known-bad boot; exits 0 when it observed the failure.
verify-negative:
	@sh scripts/verify.sh --negative

# Static checks. clippy is a hard gate for the kernel crates; the 55 user
# programs are fixtures until the own libc lands (M3.3), so theirs is advisory.
# hal and x86-smoke are M1.5's crates; the smoke path must also clippy for its
# own target, since it is no_std.
lint:
	cd os && cargo fmt --check && cargo clippy -- -D warnings
	cd elf && cargo fmt --check && cargo clippy --all-targets -- -D warnings
	cd easy-fs && cargo fmt --check && cargo clippy -- -D warnings
	cd heap && cargo fmt --check && cargo clippy --all-targets -- -D warnings
	cd hal && cargo fmt --check && cargo clippy -- -D warnings
	cd x86-smoke && cargo fmt --check && cargo clippy --target x86_64-unknown-none -- -D warnings
	cd user && cargo fmt --check && cargo clippy

.PHONY: verify verify-negative lint
