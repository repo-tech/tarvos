# Tarvos Project Cleanup Script
# Removes unnecessary build artifacts and organizes the project

# Remove build artifacts and temp files
Get-ChildItem -Recurse -Directory -Include ".build-tmp", "target", "__pycache__", "node_modules" -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force
Get-ChildItem -Recurse -File -Include "*.pdb", "*.pyc", "*.log", "*.out", "*.dump", "*.orig", "*.bak", "*.tmp", "*.swp" -ErrorAction SilentlyContinue | Remove-Item -Force
Get-ChildItem -Recurse -File -Include "dummy.*", "catching.*", "fake.*", "test_*.py", "bench_*.py", "stress_*.py" -ErrorAction SilentlyContinue | Remove-Item -Force

# Remove specific test output files that are not part of the source
Get-ChildItem -Recurse -File -Include "hello_speed.exe", "output.exe", "output.rs", "output_rust_out.exe", "output_rust_out", "tmp_app.exe", "tmp_app.pdb", "tmp_app.rs", "temp_output.rs", "demo.exe", "demo1.exe", "demo1.tarvos-manifest.json" -ErrorAction SilentlyContinue | Remove-Item -Force

# Remove extra directories
Get-ChildItem -Recurse -Directory -Include ".benchmark", ".cache", "*.egg-info" -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force

Write-Host "Cleanup complete!" -ForegroundColor Green
