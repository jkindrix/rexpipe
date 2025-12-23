# rexpipe JSON Schemas

This directory contains JSON Schema definitions for rexpipe configuration files, enabling IDE validation, autocomplete, and documentation.

## Available Schemas

### `rexpipe-pipeline.schema.json`

Validates TOML pipeline configuration files used by rexpipe.

**Features:**
- Validates step types, actions, and settings
- Provides autocomplete for all configuration options
- Documents each field with descriptions
- Catches configuration errors before runtime

## IDE Setup

### VS Code

1. Install the [Even Better TOML](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml) extension

2. Add to your `.vscode/settings.json`:
```json
{
  "evenBetterToml.schema.associations": {
    "examples/pipelines/*.toml": "./schemas/rexpipe-pipeline.schema.json",
    "*-pipeline.toml": "./schemas/rexpipe-pipeline.schema.json"
  }
}
```

### JetBrains IDEs (IntelliJ, CLion, RustRover)

1. Open Settings → Languages & Frameworks → Schemas and DTDs → JSON Schema Mappings
2. Add a new mapping:
   - Schema file: `schemas/rexpipe-pipeline.schema.json`
   - File pattern: `*-pipeline.toml` or specific paths

### Neovim (with nvim-lspconfig)

Using `taplo` LSP for TOML:

```lua
require('lspconfig').taplo.setup({
  settings = {
    taplo = {
      config = {
        schema = {
          associations = {
            [".*-pipeline\\.toml"] = "./schemas/rexpipe-pipeline.schema.json"
          }
        }
      }
    }
  }
})
```

## Direct Schema Reference

You can also reference the schema directly in your TOML file (supported by some editors):

```toml
# yaml-language-server: $schema=./schemas/rexpipe-pipeline.schema.json

name = "my-pipeline"
version = "1.0.0"

[[step]]
type = "substitute"
pattern = "\\d+"
replacement = "NUM"
```

## Contributing

When adding new configuration options to rexpipe:

1. Update the schema file with new properties
2. Include proper `description` fields for documentation
3. Add `enum` constraints where applicable
4. Test schema validation with example files
