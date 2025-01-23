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
    "--lib"
  ];

  cargoHash = "sha256-CmNKzfbc3U+GoVqjVhpmT4WNSHyc/Nwg9XhDahzvJv0=";
}
