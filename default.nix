{ nixpkgs ? null }:

# This is an example of what downstream consumers of ainglenix should do
# This is also used to dogfood as many commands as possible for ainglenix
# For example the release process for ainglenix uses this file
let
  # point this to your local config.nix file for this project
  # example.config.nix shows and documents a lot of the options
  config = import ./config.nix;

  # START AINIX IMPORT BOILERPLATE
  ainglenixPath = config.ainglenix.pathFn {};
  ainglenix = config.ainglenix.importFn {};
  # END AINIX IMPORT BOILERPLATE

  overlays = [
    (self: super: {
      inherit ainglenix ainglenixPath;

      aiToplevelDir = builtins.toString ./.;

      nixEnvPrefixEval = ''
        if [[ -n "$NIX_ENV_PREFIX" ]]; then
          # don't touch it
          :
        elif test -d "${builtins.toString self.aiToplevelDir}" &&
            test -w "${builtins.toString self.aiToplevelDir}"; then
          export NIX_ENV_PREFIX="${builtins.toString self.aiToplevelDir}"
        elif test -d "$HOME" && test -w "$HOME"; then
          export NIX_ENV_PREFIX="$HOME/.cache/aingle-dev"
          mkdir -p "$NIX_ENV_PREFIX"
        else
          export NIX_ENV_PREFIX="$(${self.coreutils}/bin/mktemp -d)"
        fi
      '';

      inherit (ainglenix.pkgs.callPackage ./nix/rust.nix { }) aiRustPlatform;
    })
  ];

  nixpkgs' = import (nixpkgs.path or ainglenix.pkgs.path) { inherit overlays; };
  inherit (nixpkgs') callPackage;

  pkgs = callPackage ./nix/pkgs/default.nix { };
in
{
  inherit
    ainglenix
    pkgs
    ;

  # TODO: refactor when we start releasing again
  # releaseHooks = callPackages ./nix/release {
  #   inherit
  #     config
  #     nixpkgs
  #     ;
  # };

  shells = callPackage ./nix/shells.nix {
    inherit
      ainglenix
      pkgs
      ;
  };
}
