# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2](https://github.com/copyleftovers/filmdrop/compare/filmdrop-cli-v0.2.1...filmdrop-cli-v0.2.2) - 2026-04-29

### Fixed

- add binstall metadata so cargo-binstall finds prebuilt binaries

## [0.2.1](https://github.com/copyleftovers/filmdrop/compare/filmdrop-cli-v0.2.0...filmdrop-cli-v0.2.1) - 2026-04-29

### Other

- update packaging metadata and URLs for copyleftovers org transfer

## [0.2.0](https://github.com/ryzhakar/gallery-rs/compare/filmdrop-cli-v0.1.6...filmdrop-cli-v0.2.0) - 2026-04-28

### Fixed

- CLI robustness — canonicalize paths, bound upload concurrency, read files once, remove dead param
- avoid unnecessary copy in process_image_from_bytes and warn on canonicalization fallback
- apply 4 targeted fixes to CLI upload pipeline

## [0.1.2](https://github.com/ryzhakar/gallery-rs/compare/filmdrop-cli-v0.1.1...filmdrop-cli-v0.1.2) - 2026-04-28

### Fixed

- swap image and manifest expiration so manifest outlives images

## [0.1.0](https://github.com/ryzhakar/gallery-rs/releases/tag/filmdrop-cli-v0.1.0) - 2026-04-28

### Fixed

- clippy useless_conversion + add pre-commit config

### Other

- cargo fmt --all
- gallery -> filmdrop across all crates, binaries, and docs
