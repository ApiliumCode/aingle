{ callPackage
, fetchFromGitHub
, makeRustPlatform
, rustChannelOf

, version ? "1.48.0"
, targets ? [ "wasm32-unknown-unknown" ]
}:

let
  aiRust = (rustChannelOf { channel = version; }).rust.override {
    inherit targets;
  };
in

{
  inherit aiRust;

  aiRustPlatform = makeRustPlatform {
    cargo = aiRust;
    rustc = aiRust;
  };
}
