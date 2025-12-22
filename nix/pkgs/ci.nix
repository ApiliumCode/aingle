{ writeShellScriptBin
, ainglenixPath
}:

{
  ciSetupNixConf = writeShellScriptBin "ai-ci-setup-nix-conf.sh" ''
    ${ainglenixPath}/ci/setup-hydra-cache.sh
  '';
}
