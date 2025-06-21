# Reset the fs_manual test area to a known state
# Defensive: ensure we never delete a critical directory by accident
$testDir = Join-Path $PSScriptRoot '..\test_playground\fs_manual'
if ([string]::IsNullOrWhiteSpace($testDir) -or $testDir.Length -lt 10) {
    throw "Refusing to operate: testDir is not set or path is suspiciously short: $testDir"
}
# Remove all contents, but not the directory itself. Use Join-Path for safety.
if (Test-Path $testDir) {
    try {
        Get-ChildItem -Path $testDir -Force | Remove-Item -Recurse -Force -ErrorAction Stop
    } catch {
        Write-Error "Failed to clean test directory: $($_)"
        exit 1
    }
} else {
    try {
        New-Item -ItemType Directory -Path $testDir -ErrorAction Stop | Out-Null
    } catch {
        Write-Error "Failed to create test directory: $($_)"
        exit 1
    }
}
# Optionally resolve the path after creation for display
try {
    $resolvedTestDir = Resolve-Path $testDir | Select-Object -ExpandProperty Path
} catch {
    $resolvedTestDir = $testDir
}
# Recreate a more complex directory structure
# Top-level
echo "Creating directory structure under $resolvedTestDir"
$dirs = @(
    'alpha', 'beta', 'gamma', 'delta',
    'alpha\sub1', 'alpha\sub2', 'alpha\sub1\deep',
    'beta\sub3', 'beta\sub3\deep2',
    'gamma\sub4', 'gamma\sub4\deep3',
    'delta\sub5'
)
foreach ($d in $dirs) {
    $full = Join-Path $testDir $d
    if (-not (Test-Path $full)) {
        try { New-Item -ItemType Directory -Path $full -ErrorAction Stop | Out-Null } catch { Write-Error "Failed to create ${full}: $($_)"; exit 1 }
    }
}
# Files at various levels
$files = @(
    @{Path='alpha\file1.txt'; Value='alpha file1'},
    @{Path='alpha\sub1\file2.txt'; Value='sub1 file2'},
    @{Path='alpha\sub1\deep\file2a.txt'; Value='deep file2a'},
    @{Path='alpha\sub2\file2b.txt'; Value='sub2 file2b'},
    @{Path='beta\file3.txt'; Value='beta file3'},
    @{Path='beta\sub3\file4.txt'; Value='sub3 file4'},
    @{Path='beta\sub3\deep2\file4a.txt'; Value='deep2 file4a'},
    @{Path='gamma\file5.txt'; Value='gamma file5'},
    @{Path='gamma\sub4\file6.txt'; Value='sub4 file6'},
    @{Path='gamma\sub4\deep3\file6a.txt'; Value='deep3 file6a'},
    @{Path='delta\file7.txt'; Value='delta file7'},
    @{Path='delta\sub5\file8.txt'; Value='sub5 file8'}
)
foreach ($f in $files) {
    $full = Join-Path $testDir $f.Path
    try { Set-Content -Path $full -Value $f.Value -ErrorAction Stop } catch { Write-Error "Failed to write ${full}: $($_)"; exit 1 }
}
# Add more files for scale
for ($i = 1; $i -le 20; $i++) {
    foreach ($bulk in @('alpha\sub1\deep', 'beta\sub3\deep2', 'gamma\sub4\deep3', 'delta\sub5')) {
        $bulkPath = Join-Path $testDir $bulk
        $file = Join-Path $bulkPath ("bulk_$i.txt")
        try { Set-Content -Path $file -Value "bulk $i" -ErrorAction Stop } catch { Write-Error "Failed to write ${file}: $($_)"; exit 1 }
    }
}
Write-Host "Test area reset: $resolvedTestDir"
