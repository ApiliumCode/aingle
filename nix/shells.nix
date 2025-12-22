{ lib
, stdenv
, mkShell
, rustup
, coreutils

, ainglenix
, aiRustPlatform
, aiToplevelDir
, nixEnvPrefixEval
, pkgs
}:

let
  aiMkShell = input: mkShell {
    # mkShell reverses the inputs list, which breaks order-sensitive shellHooks
    inputsFrom = lib.reverseList [
      { shellHook = nixEnvPrefixEval; }

      ainglenix.shell

      { shellHook = ''
        echo Using "$NIX_ENV_PREFIX" as target prefix...

        export AI_TEST_WASM_DIR="$CARGO_TARGET_DIR/.wasm_target"
        mkdir -p $AI_TEST_WASM_DIR

        export AI_WASM_CACHE_PATH="$CARGO_TARGET_DIR/.wasm_cache"
        mkdir -p $AI_WASM_CACHE_PATH
      ''; }

      input
    ];
  };
in

rec {
  # shell for AI core development. included dependencies:
  # * everything needed to compile this repos' crates
  # * CI scripts
  coreDev = aiMkShell {
    nativeBuildInputs = builtins.attrValues (pkgs.core)
      ++ [ ainglenix.pkgs.sqlcipher ];
  };

  ci = aiMkShell {
    inputsFrom = [
      (builtins.removeAttrs coreDev [ "shellHook" ])
    ];
    nativeBuildInputs = builtins.attrValues pkgs.ci;
  };

  sappDev = aiMkShell {
    inputsFrom = [
      (builtins.removeAttrs coreDev [ "shellHook" ])
    ];
    nativeBuildInputs = builtins.attrValues pkgs.happ
      ++ [ pkgs.sqlcipher ]
      ;
  };

  coreDevRustup = coreDev.overrideAttrs (attrs: {
    buildInputs = attrs.buildInputs ++ [
      rustup
    ];
  });
}
