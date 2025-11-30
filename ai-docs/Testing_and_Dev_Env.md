# Testing & Development Environment

This document outlines the testing strategies, development workflows, and CI/CD pipeline for **Markdown-Oxide**.

## 1. Testing Strategy

The project employs a multi-layered testing approach, combining unit tests for core logic with integration tests for the VS Code extension.

### Unit Tests (Rust)
*   **Location**: Co-located with source code in `src/` using the `#[test]` attribute.
*   **Coverage**:
    *   **Parsing Logic**: Extensive tests in `src/vault/mod.rs` and `src/vault/parsing.rs` verify that links (Wiki/Markdown), headings, tags, and code blocks are parsed correctly.
    *   **Configuration**: Tests in `src/config.rs` ensure that settings (including Obsidian imports) are processed correctly.
    *   **Commands**: `src/commands.rs` tests date resolution and file path generation for daily notes.
    *   **CLI**: `src/cli.rs` tests the command-line argument parsing and daily note logic.
*   **Running Tests**:
    ```bash
    cargo test
    ```

### Integration/E2E Tests (VS Code Extension)
*   **Location**: `vscode-extension/client/src/test/`.
*   **Mechanism**: Uses the standard VS Code extension testing framework (`@vscode/test-electron`).
*   **Entry Point**: `vscode-extension/scripts/e2e.sh`.
*   **Running Tests**:
    ```bash
    cd vscode-extension
    npm run test
    ```

### Test Data
*   **`TestFiles/` Directory**: Contains a sample Obsidian-like vault used likely for manual verification or integration tests. It includes:
    *   Daily notes (e.g., `2024-03-17.md`).
    *   Standard notes (`Test.md`).
    *   Configuration files (`.moxide.toml`, `.obsidian/`).
    *   Subdirectories (`folder/`, `test/`).

## 2. Development Environment

### Rust Toolchain
*   **Version**: The CI configuration specifies the **nightly** toolchain.
*   **Components**: `rustfmt` is required for style checks.

### VS Code Extension Development
*   **Dependencies**: Node.js and npm.
*   **Setup**:
    ```bash
    cd vscode-extension
    npm install
    ```
*   **Build**:
    ```bash
    npm run compile
    ```

## 3. CI/CD Pipeline

The project uses GitHub Actions for Continuous Integration (`.github/workflows/rust.yml`).

*   **Trigger**: Pushes and Pull Requests to the `main` branch.
*   **Platform**: `ubuntu-latest`.
*   **Steps**:
    1.  Checkout code.
    2.  Install **nightly** Rust toolchain with `rustfmt`.
    3.  Run `cargo fmt --check` (implied by the "Rustfmt Check" step name, though the file shows a generic usage).
    4.  `cargo build --verbose --locked`.
    5.  `cargo test --verbose`.

## 4. Key Constraints & Notes
*   **Nightly Rust**: Developers should ensure they have the nightly toolchain installed (`rustup toolchain install nightly`) to match the CI environment.
*   **VS Code Extension**: The extension wraps the binary. When developing the LSP core, you can point your local VS Code extension to the debug binary of the server for a tight feedback loop.
