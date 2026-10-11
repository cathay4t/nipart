MUSL_TARGET ?= x86_64-unknown-linux-musl

static:
	cargo build --target $(MUSL_TARGET) --release

check: check_fmt check_unit check_integ

check_integ:
	cargo build;
	sudo pytest --durations=5 -vvv -x

check_unit:
	cargo test

check_fmt:
	flake8 tests src/python
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings
