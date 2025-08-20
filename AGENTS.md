# AGENTS.md

## Build/Lint/Test Commands

```bash
# Build project
cargo build --release

# Run service
cargo run --release

# Run tests (if any)
cargo test

# Run a specific test
cargo test <test_name>

# Check code formatting
cargo fmt -- --check

# Run clippy for linting
cargo clippy -- -D warnings
```

## Code Style Guidelines

### Imports
- Group imports in order: standard library, external crates, local modules
- Use `use` statements at the top of the file
- Avoid glob imports (`use module::*`) unless specifically needed

### Formatting
- Use `cargo fmt` for automatic formatting
- Line width: 100 characters
- Indentation: 4 spaces (no tabs)

### Types
- Use explicit types for public functions and complex structures
- Prefer `Option<T>` and `Result<T, E>` for error handling
- Use strongly typed structures over generic JSON values when possible

### Naming Conventions
- Variables: snake_case
- Functions: snake_case
- Structs/Enums: PascalCase
- Constants: UPPER_SNAKE_CASE
- Modules: snake_case

### Error Handling
- Use `anyhow` for error handling in application code
- Use `thiserror` for defining custom error types
- Propagate errors with `?` operator rather than panic
- Log errors appropriately with context

### Additional Rules
- Follow existing patterns in the codebase
- Write documentation comments for public APIs
- Keep functions focused and small
- Prefer immutable data structures when possible