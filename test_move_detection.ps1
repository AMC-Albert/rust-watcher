# Test script to verify move detection improvements
# This script creates test scenarios for move detection

Write-Host "Testing move detection improvements..."

# Create test directory
$testDir = "C:\Users\Albert\Downloads\test_move_detection"
if (Test-Path $testDir) {
    Remove-Item $testDir -Recurse -Force
}
New-Item -ItemType Directory -Path $testDir | Out-Null

# Create subdirectories
$subDir1 = Join-Path $testDir "folder1"
$subDir2 = Join-Path $testDir "folder2"
New-Item -ItemType Directory -Path $subDir1 | Out-Null
New-Item -ItemType Directory -Path $subDir2 | Out-Null

# Create a test file
$testFile = Join-Path $subDir1 "test_file.txt"
"This is a test file for move detection" | Out-File -FilePath $testFile

Write-Host "Created test file: $testFile"
Write-Host "Ready to test move detection. Start the watcher with:"
Write-Host ".\watcher.exe -v --path `"$testDir`""
Write-Host ""
Write-Host "Then run these commands to test:"
Write-Host "1. Move-Item `"$testFile`" `"$subDir2`""
Write-Host "2. Rename-Item `"$(Join-Path $subDir2 "test_file.txt")`" `"renamed_file.txt`""
Write-Host ""
Write-Host "Press any key to continue and clean up..."
Read-Host

# Cleanup
Remove-Item $testDir -Recurse -Force
Write-Host "Test cleanup completed."
