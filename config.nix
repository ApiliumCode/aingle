{

 # configure ainglenix itself
 ainglenix = rec {

  # true = use a github repository as the ainglenix base (recommended)
  # false = use a local copy of ainglenix (useful for debugging)
  use-github = true;

  # controls whether ainglenix' aingle binaries (aingle, ai, etc.) are included in PATH
  includeAIngleBinaries = false;

  # configure the remote ainglenix github when use-github = true
  github = {

   # can be any github ref
   # branch, tag, commit, etc.
   ref = "refs/tags/v0.0.1";

   # the sha of what is downloaded from the above ref
   # note: even if you change the above ref it will not be redownloaded until
   #       the sha here changes (the sha is the cache key for downloads)
   # note: to get a new sha, get nix to try and download a bad sha
   #       it will complain and tell you the right sha
   sha256 = "11nmzhsrsb0x9mvk708rxsqx0i148w13dmna6y7m9g8glm9pw9ap";

   # the github owner of the ainglenix repo
   owner = "aingle";

   # the name of the ainglenix repo
   repo = "ainglenix";
  };

  # configuration for when use-github = false
  local = {
   # the path to the local ainglenix copy
   path = ../ainglenix;
  };

  pathFn = _: if use-github
     then builtins.fetchTarball (with github; {
        url = "https://github.com/AIngleLab/${repo}/archive/${ref}.tar.gz";
        inherit sha256; }
       )
     else local.path;

  importFn = _: import (pathFn {}) {
      inherit includeAIngleBinaries;
    }
    ;
 };

 release = {
  hook = {
   # sanity checks before deploying
   # to stop the release
   # exit 1
   preflight = ''
hn-release-hook-preflight-manual
'';

   # bump versions in the repo
   version = ''
hn-release-hook-version-rust
aip-release-hook-version
'';

   # publish artifacts to the world
   publish = ''
# crates are published from circle!
'';
  };

  # the commit hash that the release process should target
  # this will always be behind what ends up being deployed
  # the release process needs to add some commits for changelog etc.
  commit = "1c98fb01892f3049655e9d40765605896bac06f5";

  # the semver for prev and current releases
  # the previous version will be scanned/bumped by release scripts
  # the current version is what the release scripts bump *to*
  version = {
   current = "0.0.1";
   # not used by version hooks in this repo
   previous = "_._._";
  };

  github = {
   # markdown to inject into github releases
   # there is some basic string substitution {{ xxx }}
   # - {{ changelog }} will inject the changelog as at the target commit
   template = ''
{{ changelog }}

# Installation

Use AInix to work with this repository.

See:

- https://github.com/AIngleLab/ainglenix
- https://nixos.org/
'';

   # owner of the github repository that release are deployed to
   owner = "aingle";

   # repository name on github that release are deployed to
   repo = "aingle";

   # canonical local upstream name as per `git remote -v`
   upstream = "origin";
  };
 };
}
