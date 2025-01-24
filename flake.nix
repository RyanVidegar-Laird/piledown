{
  description = "Pileup... but down too.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    crane = {
      url = "github:ipetkov/crane";
    };

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

  outputs = { self, nixpkgs, flake-utils, crane, rust-overlay, advisory-db, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pname = "piledown";
        version = "0.1.0";
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        inherit (pkgs) lib;

        rustTarget = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };


        craneLib = (crane.mkLib pkgs).overrideToolchain rustTarget;

        # Common arguments are set here to avoid repeating them later
        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
        };

        # Build *just* the cargo dependencies, so we can reuse
        # all of that work (e.g. via cachix) when running in CI
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        individualCrateArgs = commonArgs // {
          inherit cargoArtifacts;
          inherit (craneLib.crateNameFromCargoToml { src = commonArgs.src; }) version;
          # NB: we disable tests since we'll run them all via cargo-nextest
          doCheck = false;
        };

        fileSetForCrate = crate: lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            (craneLib.fileset.commonCargoSources ./crates/libpiledown)
            (craneLib.fileset.commonCargoSources crate)
          ];
        };

        # Build the top-level crates of the workspace as individual derivations.
        # This allows consumers to only depend on (and build) only what they need.
        # Though it is possible to build the entire workspace as a single derivation,
        # so this is left up to you on how to organize things
        #
        # Note that the cargo workspace must define `workspace.members` using wildcards,
        # otherwise, omitting a crate (like we do below) will result in errors since
        # cargo won't be able to find the sources for all members.
        my-cli = craneLib.buildPackage (individualCrateArgs // {
          pname = "piledown";
          cargoExtraArgs = "-p piledown";
          src = fileSetForCrate ./crates/piledown;
        });
        my-pylib = craneLib.buildPackage (individualCrateArgs // {
          pname = "pyledown";
          cargoExtraArgs = "-p pyledown";
          src = fileSetForCrate ./crates/pyledown;
        });

        python-packages = with pkgs.python3Packages; [
          pyarrow
          seaborn
        ];
        pythonEnv = pkgs.python3.withPackages (ps: python-packages);
        devPkgs = with pkgs; [
          cargo-edit
          cargo-generate
          duckdb
          maturin
          samtools
          pyright
          ruff-lsp
          pythonEnv
        ];
        
        piledown-py = (pkgs.python3Packages.callPackage ./pkgs/piledown-py.nix {});
        pythonTestFHSEnv = pkgs.buildFHSEnv {
          name = "piledown";
          targetPkgs = pkgs: with pkgs; [
            (python3.withPackages (ps: [ piledown-py ]))
          ];
        };
      in
      {
        formatter = pkgs.nixpkgs-fmt;
        checks = {
          inherit my-cli my-pylib;

          my-workspace-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          my-workspace-doc = craneLib.cargoDoc (commonArgs // {
            inherit cargoArtifacts;
          });

          my-workspace-fmt = craneLib.cargoFmt {
            src = commonArgs.src;
          };

          my-workspace-nextest = craneLib.cargoNextest (commonArgs // {
            inherit cargoArtifacts;
            partitions = 1;
            partitionType = "count";
          });

          my-workspace-audit = craneLib.cargoAudit {
            inherit advisory-db;
            src = commonArgs.src;
          };
        };

        packages = {
          bin = pkgs.callPackage ./pkgs/piledown-bin.nix {};
          lib = pkgs.callPackage ./pkgs/piledown-lib.nix {};
          py = piledown-py;
        };

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};
          packages = devPkgs;
        };
        devShells.pyFHS = pythonTestFHSEnv.env;
      });
}
