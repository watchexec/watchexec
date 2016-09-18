debug:	src/* Cargo.toml
		@cargo build

release: src/* Cargo.toml
		@cargo build --release

clean:
		@cargo clean
