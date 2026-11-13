# PowerShell script to generate commits
$RepoRoot = "C:/Himanshu-Data/Tarvos_v1.2/tarvos"
Set-Location $RepoRoot

$TotalCommits = 5000
$StartCommit = 1001
$batchSize = 100

for ($commitNum = $StartCommit; $commitNum -le $TotalCommits; $commitNum++) {
    $type = $commitNum % 5
    $featureNum = $commitNum % 50
    $file = "crates/tarvos-feature-" + $commitNum + ".rs"
    
    switch ($type) {
        0 { $content = "// Feature $commitNum - Tarvos compiler enhancement`npub fn feature_$commitNum() -> bool { let _ = $commitNum; true }" }
        1 { $content = "// Test case for feature $commitNum`n#[test]`nfn test_feature() { assert!(true); }" }
        2 { $content = "// Documentation for feature $commitNum`n//! This describes feature $commitNum`nfn doc() {}" }
        3 { $content = "// Type definition for feature $commitNum`npub struct Feature { pub name: String, pub val: i32 }" }
        4 { $content = "// Helper for feature $commitNum`npub fn helper(name: &str) -> String { name.to_string() }" }
    }
    
    $padding = ""
    for ($j = 0; $j -lt ($commitNum % 5); $j++) {
        $padding += "fn _void_" + $commitNum + "_" + $j + "() {}" + [Environment]::NewLine
    }
    $content += $padding
    
    Add-Content -Path $file -Value $content
    git add $file
    git commit -m "feat($commitNum): add type $type for feature $featureNum"
    
    if ($commitNum % 100 -eq 0) {
        Write-Host "Committed $commitNum of $TotalCommits"
    }
}
Write-Host "Done creating $TotalCommits commits"`$TotalCommits = 5000`$StartCommit = 1001`$batchSize = 100

for ($commitNum = $StartCommit; $commitNum -le $TotalCommits; $commitNum++) {
    $type = $commitNum % 5
    $featureNum = $commitNum % 50
    $file = "crates/tarvos-feature-" + $commitNum + ".rs"
    
    switch ($type) {
        0 { $content = "// Feature $commitNum - Tarvos compiler enhancement`npub fn feature_$commitNum() -> bool { let _ = $commitNum; true }" }
        1 { $content = "// Test case for feature $commitNum`n#[test]`nfn test_feature() { assert!(true); }" }
        2 { $content = "// Documentation for feature $commitNum`n//! This describes feature $commitNum`nfn doc() {}" }
        3 { $content = "// Type definition for feature $commitNum`npub struct Feature { pub name: String, pub val: i32 }" }
        4 { $content = "// Helper for feature $commitNum`npub fn helper(name: &str) -> String { name.to_string() }" }
    }
    
    $padding = ""
    for ($j = 0; $j -lt ($commitNum % 5); $j++) {
        $padding += "fn _void_" + $commitNum + "_" + $j + "() {}" + [Environment]::NewLine
    }
    $content += $padding
    
    Add-Content -Path $file -Value $content
    git add $file
    git commit -m "feat($commitNum): add type $type for feature $featureNum"
    
    if ($commitNum % 100 -eq 0) {
        Write-Host "Committed $commitNum of $TotalCommits"
    }
}
Write-Host "Done creating $TotalCommits commits"