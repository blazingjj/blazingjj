{
  description = "TUI for Jujutsu/jj";

  nixConfig = {
    extraSubstituters = [ "https://blazingjj.cachix.org" ];
    extraTrustedPublicKeys = [
      "blazingjj.cachix.org-1:nlw0EWPJJjWMcgAcGemcBbcJOpf89K1jtILc+LNkDS0="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );

      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
    in
    {
      packages = forAllSystems (pkgs: rec {
        blazingjj = pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          inherit (cargoToml.package) version;

          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          # The TUI shells out to jj, and so do the tests.
          nativeCheckInputs = [ pkgs.jujutsu ];

          meta = {
            inherit (cargoToml.package) description;
            homepage = cargoToml.package.repository;
            license = pkgs.lib.licenses.asl20;
            mainProgram = "blazingjj";
          };
        };

        default = blazingjj;
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = [
            (pkgs.rust-bin.stable.latest.minimal.override {
              extensions = [
                "rust-src"
                "clippy"
              ];
            })
            pkgs.jujutsu
            pkgs.just
            pkgs.cargo-insta
          ];

          # rustfmt.toml uses options only nightly rustfmt accepts.
          RUSTFMT = pkgs.lib.getExe pkgs.rust-bin.nightly.latest.rustfmt;
        };
      });
    };
}
