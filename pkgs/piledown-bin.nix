{lib, rustPlatform}:

let
  fs = lib.fileset;
in
rustPlatform.buildRustPackage {
  pname = "piledown";
  version = "0.1.0";

  src = fs.toSource {
    root = ../.;
    fileset = fs.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../src
      ../assets/logo.txt
    ];
  };

  cargoBuildFlags = [
    "--bin" "piledown"
  ];

  cargoHash = "sha256-IH6Qhxh0aARwXAEi9w556NlgoKS4ZxWVdyU61m6T0Lo=";
}
