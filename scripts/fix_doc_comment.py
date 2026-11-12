path = r'C:/Himanshu-Data/Tarvos_v1.2/tarvos/crates/tarvos-cli/src/main.rs'
with open(path, 'r') as f:
    content = f.read()

# Fix: add back the missing doc comment
old = '}\nconst LONG_ABOUT: &str = "\\\\'
new = '}\n\n/// Long description shown by `tarvos --help`.\nconst LONG_ABOUT: &str = "\\\\'
content = content.replace(old, new)

with open(path, 'w') as f:
    f.write(content)
print('Done')
