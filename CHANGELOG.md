# Release Notes

All notable changes to this project will be documented in this file.

## [0.3.0] - 2025-06-17

### New
- Gossip seeds are moved to an external repo https://raw.githubusercontent.com/ackinacki/acki-nacki-igniter-seeds/refs/heads/main/seeds.yaml

### Changed
- Updated the `chitchat` crate.
- Added the `transport_layer` crate (required by chitchat).\
  Both crates were copied from the node repository.

## [0.2.1] - 2025-05-05

### Fixed
- Igniter image failed to run without `-v "/var/run/docker.sock:/var/run/docker.sock" \`

## [0.2.0] - 2025-04-29

### New
- `signatures` section in `config.yaml` with delegated licenses
- `auto_update` flag in `config.yaml` that allows to turn the auto-updates off

### Breaking changes
- `config.yaml`  has been updated with mandatory licenses list

## [0.1.0] - 2025-01-29

Initial release
