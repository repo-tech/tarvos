with open(r'C:/Himanshu-Data/Tarvos_v1.2/tarvos/.github/workflows/ci.yml', 'r') as f:
    content = f.read()

content = content.replace(
    'dtolnay/rust-toolchain@1.98.0',
    'dtolnay/rust-toolchain@stable-x86_64-pc-windows-gnu'
)

with open(r'C:/Himanshu-Data/Tarvos_v1.2/tarvos/.github/workflows/ci.yml', 'w') as f:
    f.write(content)
print('Done - replaced all occurrences')
