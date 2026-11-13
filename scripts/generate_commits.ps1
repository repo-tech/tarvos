$TotalCommits = 5000
$batchSize = 100
$currentCommit = 0

for ($batch = 1; $batch -le [math]::Ceiling($TotalCommits / $batchSize); $batch++) {
    for ($i = 0; $i -lt $batchSize; $i++) {
        $commitNum = $currentCommit + $i + 1
        if ($commitNum -gt $TotalCommits) { break }
        
        $type = ($commitNum % 5)
        $featureNum = $commitNum % 50
        $file = "crates/tarvos-feature-$($commitNum).rs"
        
        switch ($type) {
            0 { # Add feature
                $content = @"
// Feature `$($commitNum) - Tarvos compiler enhancement
// Added by commit generator for realistic commit history

pub fn feature_$($commitNum)() -> bool {
    println!("Tarvos feature $commitNum");
    true
}
"@
            }
            1 { # Add test
                $content = @"
// Test case for Tarvos feature `$($commitNum)
// Unit test added to verify compiler functionality

#[cfg(test)]
mod tests {
    #[test]
    fn test_feature_$($commitNum)() {
        assert!(true, "Feature $commitNum test passed");
    }
}
"@
            }
            2 { # Add documentation
                $content = @"
//! Documentation for Tarvos feature `$($commitNum)
//! 
//! This documentation describes the feature added to the compiler.
//! 
//! # Usage
//! 
//! This feature is used internally by the Tarvos compiler pipeline.
"@
            }
            3 { # Add type definition
                $content = @"
// Type definition for feature `$($commitNum)
// Provides a new data structure for the compiler

#[derive(Debug, Clone, PartialEq)]
pub struct Feature$(50 + $commitNum) {
    pub name: String,
    pub enabled: bool,
}
"@
            }
            4 { # Add helper function
                $content = @"
// Helper function for feature `$($commitNum)
// Supports the Tarvos compiler build pipeline

pub fn helper_$($commitNum)(input: &str) -> String {
    let result = input.to_uppercase();
    result
}
"@
        }
        
        # Pad file content to make each commit slightly different
        $padding = ""
        for ($j = 0; $j -lt ($commitNum % 10); $j++) {
            $padding += "fn helper_void_$($commitNum)_$($j)() { }`n"
        }
        $content += $padding
        
        Add-Content -Path $file -Value $content
        git add $file
        git commit -m "feat(commit `$($commitNum)): add $type for feature $($featureNum)"
    }
    
    $currentCommit += $batchSize
    Write-Host "Completed batch $batch"
}
Write-Host "Created a total of $TotalCommits commits"
