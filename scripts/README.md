# Scripts Directory

This directory contains utility scripts for the Rust filesystem watcher project.

## Git Hooks Setup

### Quick Setup
```bash
# Set the hooks directory to use versioned hooks
git config core.hooksPath scripts/hooks
```

Setting the hooks directory enables the following git hooks:

#### Pre-commit Hook
- **Auto-fix Formatting**: Automatically formats all Rust code using `rustfmt`
- **Clippy Linting**: Runs Clippy to catch potential issues and enforce best practices  
- **JSON Validation**: Validates JSON/JSONC files for syntax errors

The hook will **prevent commits** if:
- Formatting cannot be applied (rare - usually due to syntax errors)
- Clippy finds warnings or errors (fix the issues and try again)
- JSON files have syntax errors

**Note**: Formatting issues are automatically fixed during the pre-commit process.

### Bypassing Hooks

If you need to bypass the hooks (not recommended), you can use:
```bash
git commit --no-verify
```

### Hook Files

- `hooks/pre-commit`: Shell script version for Unix/Linux/macOS
- `hooks/pre-commit.ps1`: PowerShell version for Windows (called by shell script)
