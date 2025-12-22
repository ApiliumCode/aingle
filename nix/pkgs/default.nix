{ callPackage
, aiRustPlatform
, writeShellScriptBin

, aiToplevelDir
, nixEnvPrefixEval
}:

let
  aiRunCrate = writeShellScriptBin "ai-run-crate" ''
    set -x
    ${nixEnvPrefixEval}

    crate=''${1:?The first argument needs to define the crate name}
    shift
    cargo run --target-dir=''${CARGO_TARGET_DIR:?} --manifest-path=${aiToplevelDir}/crates/$crate/Cargo.toml -- $@
  '';

  ci = callPackage ./ci.nix { };
  core = callPackage ./core.nix { };
  happ = let
    mkAIngleBinaryScript = crate: writeShellScriptBin (builtins.replaceStrings ["_"] ["-"] crate) ''
      exec ${aiRunCrate}/bin/ai-run-crate ${crate} $@
    '';
  in {
    aingle = mkAIngleBinaryScript "aingle";
    ai = mkAIngleBinaryScript "ai";
  };

  all = {
    inherit
      core
      ci
      happ
      ;
  };

in builtins.mapAttrs (k: v:
  builtins.removeAttrs v [ "override" "overrideDerivation" ]
) all
