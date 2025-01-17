{
  description = "Pileup... but down too.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };

  };

  outputs = { self, nixpkgs, flake-utils, crane, rust-overlay, advisory-db, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pname = "piledown";
        version = "0.1.0";
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustTarget = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
          targets = [ "wasm32-unknown-unknown" ];
        });


        craneLib = (crane.mkLib pkgs).overrideToolchain rustTarget;

        # Common arguments are set here to avoid repeating them later
        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
        };

        # Build *just* the cargo dependencies, so we can reuse
        # all of that work (e.g. via cachix) when running in CI
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        my-crate = craneLib.buildPackage (commonArgs // {
          inherit pname version cargoArtifacts;
          
          doCheck = false;
        });


        piledown-py = pkgs.python3.pkgs.buildPythonPackage {
          inherit pname version;
          src = ./piledown;
          propagatedBuildInputs = with pkgs.python3Packages; [ pyarrow ];
          pyproject = false;
          doCheck = false;
        };

        python-packages = with pkgs.python3Packages; [
          pandas
          pyarrow
        ];
        pythonEnv = pkgs.python3.withPackages (ps: python-packages);
      in
      {
        formatter = pkgs.nixpkgs-fmt;
        checks = {
          inherit my-crate;

          my-crate-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          my-crate-doc = craneLib.cargoDoc (commonArgs // {
            inherit cargoArtifacts;
          });

          my-crate-fmt = craneLib.cargoFmt {
            src = commonArgs.src;
          };

          my-crate-nextest = craneLib.cargoNextest (commonArgs // {
            inherit cargoArtifacts;
            partitions = 1;
            partitionType = "count";
          });

          my-crate-audit = craneLib.cargoAudit {
            inherit advisory-db;
            src = commonArgs.src;
          };
        };

        packages = {
          default = my-crate;
          pypiledown = piledown-py;
        };

        apps.default = flake-utils.lib.mkApp {
          drv = my-crate;
        };

        devShells.default = craneLib.devShell {
          # Inherit inputs from checks.
          checks = self.checks.${system};

          packages = with pkgs; [
            cargo-edit
            cargo-generate
            samtools
            pythonEnv
            maturin
            pythonEnv
            # ruff-lsp
            # pyright
          ];
        };
      });
}
