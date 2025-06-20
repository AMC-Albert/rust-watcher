#!/usr/bin/env powershell
# Memory-safe test runner for rust-watcher
# This script runs tests with strict memory limits to prevent OOM crashes

param(
    [string]$TestFilter = "",
    [switch]$UnitOnly = $false,
    [switch]$IntegrationOnly = $false,
    [switch]$Verbose = $false
)

Write-Host "=== Memory-Safe Rust Test Runner ===" -ForegroundColor Green
Write-Host "Enforcing memory limits to prevent OOM crashes..." -ForegroundColor Yellow

# Set memory limits with moderate parallelism
$env:CARGO_BUILD_JOBS = "4"  # Increased from 1 to 4 for faster builds
$env:RUST_TEST_THREADS = "2" # Allow 2 test threads for faster execution
$env:RUST_BACKTRACE = "0"
$env:RUST_LOG = "error"

# Clear any existing build artifacts to free memory
Write-Host "Cleaning build artifacts..." -ForegroundColor Blue
cargo clean

if ($UnitOnly) {    Write-Host "Running unit tests only..." -ForegroundColor Blue
    if ($TestFilter) {
        cargo test --lib $TestFilter -- --test-threads=2 --nocapture
    } else {
        cargo test --lib -- --test-threads=2 --nocapture
    }
} elseif ($IntegrationOnly) {    Write-Host "Running integration tests only..." -ForegroundColor Blue
    if ($TestFilter) {
        cargo test --test $TestFilter -- --test-threads=2 --nocapture
    } else {
        cargo test --tests -- --test-threads=2 --nocapture
    }
} else {    Write-Host "Running all tests with memory safety..." -ForegroundColor Blue
    if ($TestFilter) {
        cargo test $TestFilter -- --test-threads=2 --nocapture
    } else {
        cargo test -- --test-threads=2 --nocapture
    }
}

Write-Host "Test run completed!" -ForegroundColor Green
