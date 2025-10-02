# Agents Guide for ndlm Repository

## Build, Lint, and Test Commands

- Build: `cargo build --release`
- Run all tests: `cargo test`
- Run a single test: `cargo test <test_name>`
- No explicit lint commands configured; use `cargo fmt` and `cargo clippy` if installed

## Code Style Guidelines

- Rust 2018 edition enforced (`#![deny(rust_2018_idioms)]`)
- Imports grouped by std, external crates, and internal modules
- Use `thiserror` crate for error handling with custom error enums
- Use `#[non_exhaustive]` on error enums for forward compatibility
- Naming conventions:
  - snake_case for variables, functions, and struct fields
  - PascalCase for structs, enums, and enum variants
  - CONSTANTS in uppercase with underscores
- Use explicit type annotations on struct fields
- Use `mod` declarations for internal modules
- Use lifetime annotations where applicable
- Use `impl` blocks for struct methods
- Use `Self` shorthand in constructors

## Cursor and Copilot Rules

- No `.cursor` or `.github/copilot-instructions.md` files found in this repository

---

This file is intended to guide agentic coding agents operating in this repository.