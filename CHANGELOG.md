# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Generated HTTP API route catalog at `/vault/developer/rustdoc/http/`, plus an optional explorer at `/docs` when `[server] openapi_ui` is true
- CLI reference pages on the docs site generated from clap
- Workspace rustdoc on the docs site at `/vault/developer/rustdoc/`

### Changed

- Server crate cleanup: rustdoc and HTTP API descriptions rewritten, handlers
  moved out of `server.rs`, thread-tag and contact-group CRUD unified, and
  API-token label validation typed. No behavior change.

Installable builds and release notes also appear on
[GitHub Releases](https://github.com/bitrealm-io/message-vault/releases).

The public site summary is at <https://bitrealm.io/changelog/>.
