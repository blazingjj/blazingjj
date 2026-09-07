# rustfmt.toml needs nightly rustfmt; the nix devShell points RUSTFMT at one,
# otherwise we go through rustup.
fmt := if env("RUSTFMT", "") == "" { "cargo +nightly fmt" } else { "cargo fmt" }

check:
  {{fmt}} --check
  cargo clippy
  cargo check

fix:
  {{fmt}}
  cargo clippy --fix

test:
  cargo test
