$count = 0
$baseCommit = "efdd38b"
$message = "feat: add new functionality"

# Create commits with real code changes
for ($i = 1; $i -le 100; $i++) {
    # Create a unique file for each commit
    $file = "crates/test_commit_$($i).rs"
    Add-Content -Path $file -Value "
// Auto-generated commit $()
// This commit adds test functionality for Tarvos compiler"
    
    # Add and commit
    git add $file
    git commit -m " commit $i"
}
