.PHONY: build clean

# Build the compiler and expose it as ./hulk in the repository root, as required
# by the matcom/compilers interface contract.
build:
	cargo build --release
	cp target/release/hulkc ./hulk

# Remove build artifacts and generated executables.
clean:
	cargo clean
	rm -f ./hulk ./output
