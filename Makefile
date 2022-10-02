run:
	RUST_LOG=info cargo run ./config.yml 
docs:
	cargo doc --no-deps
docs-open:
	cargo doc --no-deps --open