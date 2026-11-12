with open(r'C:/Himanshu-Data/Tarvos_v1.2/tarvos/.github/workflows/release.yml', 'r') as f:
    lines = f.readlines()

for i, line in enumerate(lines):
    if 'dtolnay/rust-toolchain@1.98.0' in line:
        print(f'--- Line {i+1} ---')
        for j in range(max(0, i-3), min(len(lines), i+4)):
            marker = '>>>' if j == i else '   '
            print(f'{marker} {j+1}: {lines[j].rstrip()}')
