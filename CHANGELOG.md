# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **MALDI-TIMS-MSI Support**: Full support for 4D imaging mass spectrometry
  - New `MaldiInfo` struct with pixel coordinates (x, y), spot names, physical positions, and laser parameters
  - Automatic detection and loading of MALDI metadata from `MaldiFrameInfo` SQLite table
  - `FrameReader::is_maldi()` method to check if dataset contains imaging data
  - Each frame now optionally contains `maldi_info` with complete spatial and laser metadata

- **Enhanced Frame Metadata**:
  - `Frame` struct now includes optional `extended_meta` field
  - Support for reading extended metadata (retention time, MS level, scan counts)
  - Improved frame metadata representation

- **Documentation**:
  - Added comprehensive module documentation
  - New example: `examples/read_tdf.rs` demonstrating dataset reading
  - Documented MALDI imaging support and usage patterns

### Changed

- `Frame` struct extended with optional `maldi_info: Option<MaldiInfo>` field
  - **Breaking**: Only if code pattern-matched on Frame struct directly
  - **Safe**: All field access through methods is backward compatible

### Fixed

- Improved error handling for missing MALDI data tables

## [0.4.2] - 2025-01-13

### Added

- Initial functionality (as per Mann Labs original version)

[Unreleased]: https://github.com/MannLabs/timsrust/compare/v0.4.2...HEAD
[0.4.2]: https://github.com/MannLabs/timsrust/releases/tag/v0.4.2
