{
  description = "Pileup... but down too.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/25.05";
    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, advisory-db, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        # CLI package (python3 needed because pyo3-build-config resolves across the workspace)
        pldn = pkgs.rustPlatform.buildRustPackage {
          pname = "pldn";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "pldn" ];
          nativeBuildInputs = [ python3 ];
          doCheck = false; # tests run separately in checks
        };

        # Python package
        python3 = pkgs.python3;
        pyledown = python3.pkgs.buildPythonPackage {
          pname = "pyledown";
          version = "0.1.0";
          src = ./.;
          pyproject = true;
          cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
            src = ./.;
            hash = "sha256-oT8fEyaAONemF8xPuzBQncI57cEba3mlffNaeKmaHmg=";
          };
          nativeBuildInputs = with pkgs.rustPlatform; [
            cargoSetupHook
            maturinBuildHook
          ];
          dependencies = with python3.pkgs; [ pyarrow ];
        };

        # Shared source for check derivations
        src = pkgs.lib.cleanSource ./.;

        pythonEnv = python3.withPackages (ps: with ps; [
          pyarrow
          seaborn
        ]);

        devPkgs = with pkgs; [
          cargo-edit
          cargo-generate
          cargo-nextest
          duckdb
          maturin
          samtools
          pyright
          ruff
          pythonEnv
        ];

      in
      {
        formatter = pkgs.nixpkgs-fmt;

        checks = {
          clippy = pkgs.rustPlatform.buildRustPackage {
            pname = "piledown-clippy";
            version = "0.1.0";
            inherit src;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ python3 pkgs.clippy ];
            doCheck = false;
            buildPhase = ''
              cargo clippy --all-targets -- --deny warnings
            '';
            installPhase = "mkdir -p $out";
          };

          fmt = pkgs.stdenv.mkDerivation {
            pname = "piledown-fmt";
            version = "0.1.0";
            inherit src;
            nativeBuildInputs = [ rustToolchain ];
            buildPhase = ''
              cargo fmt -- --check
            '';
            installPhase = "mkdir -p $out";
          };

          nextest = pkgs.rustPlatform.buildRustPackage {
            pname = "piledown-nextest";
            version = "0.1.0";
            inherit src;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ pkgs.cargo-nextest python3 ];
            doCheck = false;
            buildPhase = ''
              cargo nextest run
            '';
            installPhase = "mkdir -p $out";
          };

          audit = pkgs.stdenv.mkDerivation {
            pname = "piledown-audit";
            version = "0.1.0";
            inherit src;
            nativeBuildInputs = [ rustToolchain pkgs.cargo-audit ];
            buildPhase = ''
              HOME=$TMPDIR cargo audit --db ${advisory-db} --no-fetch \
                --ignore RUSTSEC-2025-0024 \
                --ignore RUSTSEC-2025-0020
            '';
            installPhase = "mkdir -p $out";
          };

          doc = pkgs.rustPlatform.buildRustPackage {
            pname = "piledown-doc";
            version = "0.1.0";
            inherit src;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ python3 ];
            doCheck = false;
            buildPhase = ''
              cargo doc --no-deps
            '';
            installPhase = "mkdir -p $out";
          };
        };

        packages = {
          default = pldn;
          inherit pldn pyledown;
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [ rustToolchain ] ++ devPkgs;
        };
      });
}
