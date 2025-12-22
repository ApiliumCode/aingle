{ pkgs, config }:
let
 name = "aip-release-hook-version";

 script = pkgs.writeShellScriptBin name ''
echo "bumping aingle_2020 dependency versions to ${config.release.version.current} in all Cargo.toml"
find . \
 -name "Cargo.toml" \
 -not -path "**/target/**" \
 -not -path "**/.git/**" \
 -not -path "**/.cargo/**" | xargs -I {} \
 sed -i 's/^aingle_2020 = { version = "=[0-9]\+.[0-9]\+.[0-9]\+\(-alpha[0-9]\+\)\?"/aingle_2020 = { version = "=${config.release.version.current}"/g' {}
'';
in
{
 buildInputs = [ script ];
}
