# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/ryzhakar/gallery-rs/compare/filmdrop-core-v0.1.6...filmdrop-core-v0.2.0) - 2026-04-28

### Fixed

- apply 4 targeted fixes to CLI upload pipeline

## [0.1.5](https://github.com/ryzhakar/gallery-rs/compare/filmdrop-core-v0.1.4...filmdrop-core-v0.1.5) - 2026-04-28

### Fixed

- paginate list_objects_v2 in delete_prefix to handle >1000 objects

## [0.1.3](https://github.com/ryzhakar/gallery-rs/compare/filmdrop-core-v0.1.2...filmdrop-core-v0.1.3) - 2026-04-28

### Fixed

- propagate non-404 errors in S3Client::object_exists

## [0.1.0](https://github.com/ryzhakar/gallery-rs/releases/tag/filmdrop-core-v0.1.0) - 2026-04-28

### Other

- cargo fmt --all
- gallery -> filmdrop across all crates, binaries, and docs
