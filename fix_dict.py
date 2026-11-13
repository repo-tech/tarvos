with open('C:/Himanshu-Data/Tarvos_v1.2/tarvos/crates/tarvos-codegen-rust/src/lib.rs', 'r', encoding='utf-8') as f:
    content = f.read()

old = """                    name if name.starts_with("tarvos_list_") => {
                        Self::emit_list_builtin(name, &args_rendered)?
                    }

                    name if name.starts_with("tarvos_dict_") => {
                        Self::emit_dict_builtin(name, &args_rendered)?
                    }
                    "tarvos_os_getcwd" => {"""
new = """                    name if name.starts_with("tarvos_list_") => {
                        Self::emit_list_builtin(name, &args_rendered)?
                    }
                    name if name.starts_with("tarvos_dict_") => {
                        Self::emit_dict_builtin(name, &args_rendered)?
                    }
                    "tarvos_os_getcwd" => {"""
if old in content:
    content = content.replace(old, new)
    with open('C:/Himanshu-Data/Tarvos_v1.2/tarvos/crates/tarvos-codegen-rust/src/lib.rs', 'w', encoding='utf-8') as f:
        f.write(content)
    print("Fixed")
else:
    print("Not found")