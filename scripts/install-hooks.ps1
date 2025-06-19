# Git Hooks Setup Script
# This script configures git to use versioned hooks from scripts/hooks/

$ErrorActionPreference = "Stop"

Write-Host "🔧 Setting up Git hooks for Rust filesystem watcher..." -ForegroundColor Cyan

# Get the repository root
$repoRoot = Split-Path -Parent $PSScriptRoot
$hooksPath = "scripts/hooks"

# Check if we're in a git repository
if (-not (Test-Path (Join-Path $repoRoot ".git"))) {
    Write-Host "❌ Error: Not in a git repository root" -ForegroundColor Red
    exit 1
}

# Check if hooks directory exists
$hooksDir = Join-Path $repoRoot $hooksPath
if (-not (Test-Path $hooksDir)) {
    Write-Host "❌ Error: Hooks directory not found: $hooksPath" -ForegroundColor Red
    exit 1
}

# Configure git to use our hooks directory
Write-Host "📋 Configuring git to use versioned hooks..." -ForegroundColor Yellow
git config core.hooksPath $hooksPath

if ($LASTEXITCODE -eq 0) {
    Write-Host "✅ Git hooks configured successfully!" -ForegroundColor Green
    Write-Host ""
    Write-Host "📁 Hooks directory: $hooksPath" -ForegroundColor White
    Write-Host "🔍 Active hooks:" -ForegroundColor White
    
    # List available hooks
    $hookFiles = Get-ChildItem -Path $hooksDir -File
    foreach ($hookFile in $hookFiles) {
        Write-Host "  • $($hookFile.Name)" -ForegroundColor White
    }
    
    Write-Host ""
    Write-Host "💡 Hooks will run automatically on git operations" -ForegroundColor Cyan
    Write-Host "🚫 To bypass hooks (not recommended): git commit --no-verify" -ForegroundColor Yellow
} else {
    Write-Host "❌ Failed to configure git hooks" -ForegroundColor Red
    exit 1
}
