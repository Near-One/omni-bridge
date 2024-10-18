.PHONY: rust-lint rust-lint-near rust-lint-omni-relayer

OPTIONS = -D warnings -D clippy::pedantic -A clippy::missing_errors_doc -A clippy::must_use_candidate -A clippy::module_name_repetitions

rust-lint: rust-lint-near rust-lint-relayer

rust-lint-near:
	cargo clippy --manifest-path ./near/Cargo.toml -- $(OPTIONS)

rust-lint-omni-relayer:
	cargo clippy --manifest-path ./omni-relayer/Cargo.toml -- $(OPTIONS)
