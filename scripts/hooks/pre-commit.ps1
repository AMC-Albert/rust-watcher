# Pre-commit hook for Rust projects (PowerShell version)
# This hook runs formatting checks and clippy before allowing commits

$ErrorActionPreference = "Stop"

Write-Host "Running pre-commit checks..." -ForegroundColor Cyan

# Check if cargo is available
try {
    cargo --version | Out-Null
} catch {
    Write-Host "Error: cargo not found. Please install Rust." -ForegroundColor Red
    exit 1
}

# 1. Check formatting
Write-Host "Checking code formatting..." -ForegroundColor Yellow
try {
    cargo fmt --check
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Code formatting issues found!" -ForegroundColor Red
        Write-Host "Run 'cargo fmt' to fix formatting, then commit again." -ForegroundColor Blue
        exit 1
    }
} catch {
    Write-Host "Formatting check failed: $_" -ForegroundColor Red
    exit 1
}
Write-Host "Code formatting is correct" -ForegroundColor Green

# 2. Run clippy
Write-Host "Running clippy..." -ForegroundColor Yellow
try {
    cargo clippy --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Clippy found issues!" -ForegroundColor Red
        Write-Host "Fix clippy warnings and try committing again." -ForegroundColor Blue
        exit 1
    }
} catch {
    Write-Host "Clippy check failed: $_" -ForegroundColor Red
    exit 1
}
Write-Host "Clippy checks passed" -ForegroundColor Green

# 3. Check JSON files formatting and validation
Write-Host "Checking JSON files..." -ForegroundColor Yellow
$jsonFiles = Get-ChildItem -Path . -Recurse -Include "*.json" | Where-Object { $_.FullName -notlike "*target*" -and $_.FullName -notlike "*node_modules*" }
if ($jsonFiles.Count -gt 0) {
    try {
        foreach ($file in $jsonFiles) {
            # Test if JSON is valid by attempting to parse it (skip .vscode files as they may have comments)
            if ($file.FullName -notlike "*.vscode*") {
                $content = Get-Content $file.FullName -Raw
                if ($content) {
                    $null = ConvertFrom-Json $content -ErrorAction Stop
                }
            }
        }
        Write-Host "JSON files are valid" -ForegroundColor Green
    } catch {
        Write-Host "JSON validation failed for file: $($file.FullName)" -ForegroundColor Red
        Write-Host "Error: $_" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "No JSON files found to validate" -ForegroundColor Gray
}

Write-Host "All pre-commit checks passed! Proceeding with commit..." -ForegroundColor Green
