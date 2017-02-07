LATEST_TAG=$(shell git tag | tail -n1)

.PHONY: doc test

debug:	src/* Cargo.toml
	@cargo build

release: src/* Cargo.toml
	@cargo build --release

clean:
	@cargo clean

test:
	@cargo test

doc: doc/watchexec.1.ronn
	@ronn doc/watchexec.1.ronn

cargo-release:
	@cargo publish

homebrew-release:
	@brew bump-formula-pr --strict --url="https://github.com/mattgreen/watchexec/archive/$(LATEST_TAG).tar.gz" watchexec

install: release
	@cp target/release/watchexec /usr/bin

uninstall:
	@rm /usr/bin/watchexec
