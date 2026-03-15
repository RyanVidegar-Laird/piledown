{
  description = "Pileup... but down too.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/25.11";
    flake-utils.url = "github:numtide/flake-utils";

  };

  outputs = { self, nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        # CLI package (python3 needed because pyo3-build-config resolves across the workspace)
        pldn = pkgs.rustPlatform.buildRustPackage {
          pname = "pldn";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "pldn" ];
          nativeBuildInputs = [ python3 pkgs.R ];
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
            hash = "sha256-B++Ma11T2b7jSB0tEm1y6Z6YInqcUusG7v2qk4O0od0=";
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

        rEnv = pkgs.rWrapper.override {
          packages = with pkgs.rPackages; [
            arrow
            nanoarrow
            devtools
            roxygen2
            rextendr
            testthat
          ];
        };

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
          rEnv
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
            nativeBuildInputs = [ python3 pkgs.R pkgs.clippy ];
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
            nativeBuildInputs = [ pkgs.rustfmt pkgs.cargo ];
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
            nativeBuildInputs = [ pkgs.cargo-nextest python3 pkgs.R ];
            doCheck = false;
            buildPhase = ''
              cargo nextest run
            '';
            installPhase = "mkdir -p $out";
          };

          # audit check temporarily disabled: cargo-audit in nixpkgs 25.11
          # doesn't support CVSS 4.0 format used in current advisory-db.
          # Re-enable once nixpkgs ships cargo-audit >= 0.22.

          doc = pkgs.rustPlatform.buildRustPackage {
            pname = "piledown-doc";
            version = "0.1.0";
            inherit src;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ python3 pkgs.R ];
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
          nativeBuildInputs = [ pkgs.rustc pkgs.cargo pkgs.clippy pkgs.rustfmt pkgs.rust-analyzer ] ++ devPkgs;
        };
      });
}
