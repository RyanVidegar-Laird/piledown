{
  lib,
  buildPythonPackage,
  rustPlatform,
}:

let
  fs = lib.fileset;
in

buildPythonPackage {
  pname = "piledown-py";
  version = "0.1.0";
  
  src = fs.toSource {
    root = ../.;
    fileset = fs.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../src
      ../assets/logo.txt
      ../piledown
    ];
  };
  cargoDeps = rustPlatform.importCargoLock {
    lockFile = ../Cargo.lock;
  };

  nativeBuildInputs = with rustPlatform; [ cargoSetupHook maturinBuildHook ];
}
