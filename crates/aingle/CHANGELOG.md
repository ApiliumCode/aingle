# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

*Note: Versions 0.0.52-alpha2 and older are part belong to previous iterations of the AIngle architecture and are not tracked here.*

## Unreleased

### Added

- `InstallAppBundle` command added to admin conductor API. [#665](https://github.com/ApiliumCode/aingle/pull/665)
- `SafSource` in conductor_api `RegisterSaf` call now can take a `SafBundle` [#665](https://github.com/ApiliumCode/aingle/pull/665)

### Removed

- BREAKING:  `InstallAppSafPayload` in admin conductor API `InstallApp` command now only accepts a hash.  Both properties and path have been removed as per deprecation warning.  Use either `RegisterSaf` or `InstallAppBundle` instead. [#665](https://github.com/ApiliumCode/aingle/pull/665)
- BREAKING: `SafSource(Path)` in conductor_api `RegisterSaf` call now must point to `SafBundle` as created by `ai saf pack` not a `SafFile` created by `saf_util` [#665](https://github.com/ApiliumCode/aingle/pull/665)

## 0.0.100

This is the first version number for the version of AIngle with a refactored state model (you may see references to it as AIngle RSM).
