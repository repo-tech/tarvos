$commitCount = 100
for ($i = 1; $i -le $commitCount; $i++) {
    $file = "crates/tarvos-$($i).rs"
    Add-Content -Path $file -Value "
// Feature $() - Added by commit generator script
// This module provides functionality for Tarvos compiler test suite

#[allow(unused)]
pub fn commit_feature_() {
    let _feature = true;
    println!("Feature {} initialized", );
    $i
}
"
    git add $file
    git commit -m "feat(commit $i): add feature  for compiler testing"
}
Write-Host "Created $commitCount commits"
