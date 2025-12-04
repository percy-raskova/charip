---
title: CLI Reference
---

# Command-Line Interface

charip-lsp provides both an LSP server and CLI utilities.

## Running the LSP Server

The default mode runs the LSP server:

```bash
charip
```

The server communicates over stdin/stdout using the LSP protocol.

## CLI Commands

### `charip daily`

Open or create today's daily note.

```bash
charip daily
```

Behavior:
1. Calculates today's date using configured format
2. If the file exists, opens it
3. If not, creates it from template (if configured)

Uses settings:
- `daily_notes_folder`: Where daily notes are stored
- `dailynote`: Date format string (e.g., `%Y-%m-%d`)

### `charip config`

Open the configuration file.

```bash
charip config
```

Opens the first found:
1. `.moxide.toml` in current directory
2. `~/.config/moxide/settings.toml`

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Configuration error |

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `MOXIDE_LOG` | Log level (error, warn, info, debug, trace) |

Example:

```bash
MOXIDE_LOG=debug charip
```

## LSP Communication

When running as an LSP server, charip-lsp:

- Reads JSON-RPC messages from stdin
- Writes JSON-RPC responses to stdout
- Logs to stderr (when enabled)

### Initialization

The client must send `initialize` with the workspace root:

```json
{
  "rootUri": "file:///path/to/documentation"
}
```

charip-lsp uses this to locate `conf.py` and index the vault.

## For Editor Plugin Authors

### Spawning the Server

```javascript
const server = spawn('charip', [], {
  stdio: ['pipe', 'pipe', 'pipe']
});
```

### Root Detection

charip-lsp looks for these markers to identify the vault root:
1. `conf.py` (Sphinx project)
2. `.git` directory
3. `_toc.yml` (Jupyter Book)

### Capabilities

On `initialize`, charip-lsp reports its capabilities:

```json
{
  "capabilities": {
    "completionProvider": {
      "triggerCharacters": ["[", "{", " ", "(", "#", ">"]
    },
    "definitionProvider": true,
    "referencesProvider": true,
    "hoverProvider": true,
    "renameProvider": true,
    "documentSymbolProvider": true,
    "workspaceSymbolProvider": true
  }
}
```
