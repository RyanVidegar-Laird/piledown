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
            hash = "sha256-luI7IKndf9GQCYXNnrZvPdRjxRL0LsUdgS5nFTFYTPA=";
          };
          nativeBuildInputs = with pkgs.rustPlatform; [
            cargoSetupHook
            maturinBuildHook
          ];
          dependencies = with python3.pkgs; [ pyarrow ];
        };

        # R package — uses full repo src so cargo can find workspace root
        piledownR = pkgs.rPackages.buildRPackage {
          name = "piledownR";
          src = ./.;
          postUnpack = ''
            # cargoSetupHook expects Cargo.lock at sourceRoot.
            # Copy it from the workspace root before we narrow sourceRoot.
            cp "$sourceRoot/Cargo.lock" "$sourceRoot/crates/piledownR/Cargo.lock"
            sourceRoot="$sourceRoot/crates/piledownR"
          '';
          cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
            src = ./.;
            hash = "sha256-luI7IKndf9GQCYXNnrZvPdRjxRL0LsUdgS5nFTFYTPA=";
          };
          nativeBuildInputs = with pkgs; [
            rustPlatform.cargoSetupHook
            cargo
            rustc
          ];
          propagatedBuildInputs = with pkgs.rPackages; [ arrow nanoarrow ];
          # cargoSetupHook sets up vendoring at the workspace root level.
          # We need to ensure it finds Cargo.lock in the full repo.
          postPatch = ''
            patchShebangs configure
          '';
          preBuild = ''
            export CARGO_HOME=$TMPDIR/.cargo
          '';
        };

        # Shared source for check derivations
        src = pkgs.lib.cleanSource ./.;

        pythonEnv = python3.withPackages (ps: with ps; [
          pyarrow
          pandas
          pytest
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
            piledownR
            ggplot2
            tidyr
            dplyr
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
              cargo nextest run --workspace --exclude pyledown
            '';
            installPhase = "mkdir -p $out";
          };

          # audit check temporarily disabled: cargo-audit in nixpkgs 25.11
          # doesn't support CVSS 4.0 format used in current advisory-db.
          # Re-enable once nixpkgs ships cargo-audit >= 0.22.

          piledownR-integration = pkgs.stdenv.mkDerivation {
            pname = "piledownR-integration";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;
            nativeBuildInputs = [
              (pkgs.rWrapper.override {
                packages = [ piledownR pkgs.rPackages.testthat pkgs.rPackages.arrow pkgs.rPackages.nanoarrow ];
              })
            ];
            buildPhase = ''
              cd crates/piledownR
              Rscript -e "library(piledownR); testthat::test_dir('tests/testthat', stop_on_failure = TRUE)"
            '';
            installPhase = "mkdir -p $out";
          };

          pyledown-integration = pkgs.stdenv.mkDerivation {
            pname = "pyledown-integration";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;
            nativeBuildInputs = [
              (python3.withPackages (ps: [
                pyledown
                ps.pandas
                ps.pytest
              ]))
            ];
            buildPhase = ''
              pytest tests/python/ -v
            '';
            installPhase = "mkdir -p $out";
          };

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
          inherit pldn pyledown piledownR;
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [ pkgs.rustc pkgs.cargo pkgs.clippy pkgs.rustfmt pkgs.rust-analyzer ] ++ devPkgs;
        };
      });
}
