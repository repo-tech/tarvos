// tarvos-tests: Integration tests for the full compiler pipeline.

/// Re-export nothing â€” this crate is test-only.
pub fn _placeholder() {}

#[cfg(test)]
mod pipeline {
    use tarvos_analysis::lower_module;
    use tarvos_codegen_rust::RustCodegen;
    use tarvos_optimizer::Optimizer;
    use tarvos_parser::parse_python_ast;

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Helpers
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Run the full pipeline: JSON AST â†’ Rust source string.
    fn compile(ast_json: &str) -> Result<String, String> {
        let module = parse_python_ast(ast_json).map_err(|e| e.to_string())?;
        let ir = lower_module(&module).map_err(|e| e.to_string())?;
        let ir = Optimizer::optimize(&ir).map_err(|e| e.to_string())?;
        RustCodegen::generate(&ir).map_err(|e| e.to_string())
    }

    /// A tuple assignment rebinds its targets, so the constant `a = 0` must not
    /// survive into `return a`.
    ///
    /// This is the Fibonacci shape, and it is a real correctness regression: with
    /// `Destructure` missing from the optimizer's mutation set, copy propagation
    /// kept the literal and the generated Rust read `return 0_i64`. The program
    /// still compiled and ran, so nothing but a parity check would have caught
    /// it. `a, b = 0, 1` on one line happened to work, which is why it survived:
    /// only the separate `a = 0` then `b = 1` form reproduced it.
    #[test]
    fn a_loop_rebinding_through_tuple_assignment_is_not_folded_away() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"funcdef","name":"fib",
                 "args":["n"],
                 "arg_annotations":["int"],
                 "returns":"int",
                 "body":[
                    {"type":"assign","target":{"type":"name","id":"a"},"value":{"type":"int","value":0}},
                    {"type":"assign","target":{"type":"name","id":"b"},"value":{"type":"int","value":1}},
                    {"type":"for","target":{"type":"name","id":"i"},
                     "iter":{"type":"call","function":{"type":"name","id":"range"},
                             "args":[{"type":"name","id":"n"}],"keywords":[]},
                     "body":[
                      {"type":"assign",
                       "target":{"type":"tuple","elements":[{"type":"name","id":"a"},{"type":"name","id":"b"}]},
                       "value":{"type":"tuple","elements":[
                          {"type":"name","id":"b"},
                          {"type":"binary","left":{"type":"name","id":"a"},"operator":"add","right":{"type":"name","id":"b"}}
                       ]}}
                     ]},
                    {"type":"return","value":{"type":"name","id":"a"}}
                 ]}
            ]
        }"#;
        let code = compile(ast).expect("tuple assignment in a loop should lower");
        assert!(
            code.contains("return a;"),
            "`return a` must return the variable, not a folded literal:\n{code}"
        );
        assert!(
            !code.contains("return 0_i64;"),
            "the initial `a = 0` leaked past the tuple assignment that rebinds it:\n{code}"
        );
    }

    #[test]
    fn numeric_pow_append_len_and_break_lower() {
        let ast = r#"{"type":"module","body":[
          {"type":"assign","target":{"type":"name","id":"xs"},"value":{"type":"list","elements":[{"type":"int","value":2}]}},
          {"type":"expr","value":{"type":"method_call","object":{"type":"name","id":"xs"},"method":"append","args":[{"type":"int","value":3}]}},
          {"type":"while","test":{"type":"bool","value":true},"body":[{"type":"break"}]},
          {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},"args":[
            {"type":"binary","left":{"type":"int","value":2},"operator":"pow","right":{"type":"int","value":3}},
            {"type":"call","function":{"type":"name","id":"len"},"args":[{"type":"name","id":"xs"}]}
          ]}}
        ]}"#;
        let code = compile(ast).expect("new subset features should lower");
        assert_contains_all(
            &code,
            &["xs.push(3_i64);", "break;", "checked_pow", ".len()"],
        );
    }

    #[test]
    fn dictionary_literal_lookup_and_string_key_update_lower() {
        let ast = r#"{"type":"module","body":[
          {"type":"assign","target":{"type":"name","id":"scores"},"value":{"type":"dict",
            "keys":[{"type":"string","value":"alice"},{"type":"string","value":"bob"}],
            "values":[{"type":"int","value":10},{"type":"int","value":20}]}},
          {"type":"assign","target":{"type":"subscript","value":{"type":"name","id":"scores"},"index":{"type":"string","value":"bob"}},
            "value":{"type":"binary","left":{"type":"subscript","value":{"type":"name","id":"scores"},"index":{"type":"string","value":"bob"}},
              "operator":"add","right":{"type":"int","value":5}}},
          {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},"args":[
            {"type":"subscript","value":{"type":"name","id":"scores"},"index":{"type":"string","value":"bob"}}]}}
        ]}"#;
        let code = compile(ast).expect("dictionary subset should lower");
        assert_contains_all(&code, &["HashMap::from", ".insert(", "scores[&"]);
    }

    #[test]
    fn static_json_dumps_lowers_to_native_string() {
        let ast = r#"{"type":"module","body":[
          {"type":"import","names":[{"name":"json","asname":null}]},
          {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},"args":[
            {"type":"method_call","object":{"type":"name","id":"json"},"method":"dumps","args":[
              {"type":"dict","keys":[{"type":"string","value":"message"},{"type":"string","value":"items"}],
               "values":[{"type":"string","value":"hello\nworld"},{"type":"list","elements":[
                 {"type":"int","value":1},{"type":"bool","value":true}
               ]}]}
            ]}]}}
        ]}"#;
        let code = compile(ast).expect("static json.dumps should lower natively");
        assert!(
            code.contains(r#"\"message\":\"hello\\nworld\""#)
                && code.contains(r#"\"items\":[1,true]"#),
            "generated JSON literal missing or incorrectly escaped:\n{}",
            code
        );
    }

    /// A `json.dumps` on a value the program built at run time is serialized
    /// by the runtime, not rejected. Only the compile-time literal is rendered
    /// during lowering, because that one is exact and needs no runtime.
    #[test]
    fn dynamic_json_dumps_uses_the_runtime_serializer() {
        let ast = r#"{"type":"module","body":[
          {"type":"import","names":[{"name":"json","asname":null}]},
          {"type":"assign","target":{"type":"name","id":"payload"},"value":{"type":"name","id":"runtime_value"}},
          {"type":"expr","value":{"type":"method_call","object":{"type":"name","id":"json"},"method":"dumps","args":[
            {"type":"name","id":"payload"}
          ]}}
        ]}"#;
        let code = compile(ast).expect("json.dumps on a runtime value must lower");
        assert!(
            code.contains("__tarvos_json()"),
            "runtime json.dumps must route through the serializer:\n{code}"
        );
        assert!(
            code.contains("fn __tarvos_json(&self)"),
            "the JSON runtime must be emitted into the program:\n{code}"
        );
    }

    #[test]
    fn top_level_function_call_statement_is_emitted() {
        let ast = r#"{"type":"module","body":[
          {"type":"funcdef","name":"run_benchmark","args":[],"arg_annotations":[],"body":[
            {"type":"return","value":null}
          ],"returns":null},
          {"type":"expr","value":{"type":"call","function":{"type":"name","id":"run_benchmark"},"args":[]}}
        ]}"#;
        let code = compile(ast).expect("function call statement should lower");
        assert_contains_all(&code, &["fn run_benchmark()", "run_benchmark();"]);
    }

    #[test]
    fn nested_loop_boolean_assignments_keep_boolean_storage() {
        let ast = r#"{"type":"module","body":[
          {"type":"try","body":[
            {"type":"assign","target":{"type":"name","id":"primes"},"value":{"type":"list","elements":[]}},
            {"type":"for","target":{"type":"name","id":"num"},
             "iter":{"type":"call","function":{"type":"name","id":"range"},
                     "args":[{"type":"int","value":2},{"type":"int","value":5}],"keywords":[]},
             "body":[
               {"type":"assign","target":{"type":"name","id":"is_prime"},"value":{"type":"bool","value":true}},
               {"type":"for","target":{"type":"name","id":"i"},
                "iter":{"type":"call","function":{"type":"name","id":"range"},
                        "args":[{"type":"int","value":2},{"type":"int","value":3}],"keywords":[]},
                "body":[
                  {"type":"if","test":{"type":"compare","left":{"type":"name","id":"num"},
                      "operators":["eq"],"comparators":[{"type":"name","id":"i"}]},
                   "body":[{"type":"assign","target":{"type":"name","id":"is_prime"},
                            "value":{"type":"bool","value":false}}],"orelse":[]}
                ]}
             ]}],
            "handlers":[],"orelse":[],"finalbody":[]}
        ]}"#;
        let code = compile(ast).expect("nested boolean assignments should lower");
        assert_contains_all(
            &code,
            &[
                "let mut is_prime = false;",
                "is_prime = true;",
                "is_prime = false;",
            ],
        );
        assert!(!code.contains("let mut is_prime = 0_i64;"));
    }

    /// A name bound to two different types is boxed, not rejected.
    ///
    /// This is the exact program `x = 1` then `x = "dynamic"` produces, and
    /// it used to be refused with a message telling the user to fall back to
    /// CPython. It now compiles natively as a tagged value, so the test pins
    /// the emitted code: what matters is that both bindings name one Rust type.
    #[test]
    fn mixed_native_reassignment_becomes_a_tagged_value() {
        let ast = r#"{"type":"module","body":[
          {"type":"assign","target":{"type":"name","id":"value"},"value":{"type":"int","value":1}},
          {"type":"assign","target":{"type":"name","id":"value"},"value":{"type":"string","value":"dynamic"}},
          {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
           "args":[{"type":"name","id":"value"}],"keywords":[]}}
        ]}"#;
        let code = compile(ast).expect("a type-changing name compiles natively");
        assert_contains_all(
            &code,
            &[
                "let mut value = __TarvosValue::",
                "value = __TarvosValue::Str(",
            ],
        );
    }

    #[test]
    fn incompatible_branch_types_request_python_fallback() {
        let ast = r#"{"type":"module","body":[
          {"type":"if","test":{"type":"compare","left":{"type":"int","value":1},
           "operators":["lt"],"comparators":[{"type":"name","id":"condition"}]},
           "body":[{"type":"assign","target":{"type":"name","id":"value"},
                    "value":{"type":"bool","value":true}}],
           "orelse":[{"type":"assign","target":{"type":"name","id":"value"},
                      "value":{"type":"int","value":1}}]},
          {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
           "args":[{"type":"name","id":"value"}],"keywords":[]}}
        ]}"#;
        let error = compile(ast).expect_err("incompatible branch types must be explicit");
        assert!(error.contains("branch `value` changes from"));
        assert!(error.contains("--python-fallback"));
    }

    #[test]
    fn filtered_list_comprehension_lowers_to_collect() {
        let ast = r#"{"type":"module","body":[
          {"type":"assign","target":{"type":"name","id":"values"},"value":{
            "type":"list_comp",
            "elt":{"type":"binary","left":{"type":"name","id":"x"},"operator":"mul","right":{"type":"int","value":2}},
            "target":"x",
            "iter":{"type":"call","function":{"type":"name","id":"range"},"args":[{"type":"int","value":6}],"keywords":[]},
            "condition":{"type":"compare","left":{"type":"name","id":"x"},"operators":["gt"],"comparators":[{"type":"int","value":2}]}
          }},
          {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},"args":[
            {"type":"call","function":{"type":"name","id":"len"},"args":[{"type":"name","id":"values"}],"keywords":[]}
          ],"keywords":[]}}
        ]}"#;
        let code = compile(ast).expect("list comprehension should lower");
        assert_contains_all(
            &code,
            &[
                ".into_iter().filter_map",
                ".collect::<Vec<_>>()",
                "values.len()",
            ],
        );
    }

    #[test]
    fn unsupported_imports_fail_explicitly() {
        let ast = r#"{"type":"module","body":[
          {"type":"import","names":[{"name":"numpy","asname":null}]}
        ]}"#;
        let error = compile(ast).expect_err("unsupported native imports must fail");
        assert!(error.contains("not supported by the native backend"));
    }

    #[test]
    fn os_path_imports_lower_to_native_filesystem_calls() {
        let ast = r#"{"type":"module","body":[
          {"type":"import","names":[{"name":"os.path","asname":null}]},
          {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},"args":[
            {"type":"method_call","object":{"type":"attribute","value":{"type":"name","id":"os"},"attr":"path"},
             "method":"join","args":[{"type":"string","value":"tmp"},{"type":"string","value":"file.txt"}]}
          ],"keywords":[]}}
        ]}"#;
        let code = compile(ast).expect("os.path call should lower");
        assert_contains_all(
            &code,
            &["PathBuf::from", ".join(", "to_string_lossy().into_owned()"],
        );
    }

    #[test]
    fn class_instances_store_fields_and_mutate_through_methods() {
        let ast = r#"{"type":"module","body":[
          {"type":"classdef","name":"Counter","bases":[],"body":[
            {"type":"funcdef","name":"__init__","args":["self","start"],
             "arg_annotations":[null,"int"],
             "body":[
               {"type":"assign","target":{"type":"attribute","value":{"type":"name","id":"self"},"attr":"value"},
                "value":{"type":"name","id":"start"}}
             ],"returns":null},
            {"type":"funcdef","name":"next","args":["self"],"arg_annotations":[null],
             "body":[
               {"type":"assign","target":{"type":"attribute","value":{"type":"name","id":"self"},"attr":"value"},
                "value":{"type":"binary","left":{"type":"attribute","value":{"type":"name","id":"self"},"attr":"value"},
                 "operator":"add","right":{"type":"int","value":1}}},
               {"type":"return","value":{"type":"attribute","value":{"type":"name","id":"self"},"attr":"value"}}
             ],"returns":"int"}
          ]},
          {"type":"assign","target":{"type":"name","id":"counter"},
           "value":{"type":"call","function":{"type":"name","id":"Counter"},
            "args":[{"type":"int","value":4}],"keywords":[]}},
          {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
           "args":[{"type":"method_call","object":{"type":"name","id":"counter"},"method":"next","args":[]}],
           "keywords":[]}}
        ]}"#;
        let code = compile(ast).expect("class instance fields should lower");
        assert_contains_all(
            &code,
            &[
                "struct Counter",
                "value: i64",
                "fn __tarvos_ctor_counter",
                "self_obj.value",
                "fn counter_next",
                "&mut counter",
            ],
        );
    }

    #[test]
    fn loops_over_strings_and_dictionary_keys_with_native_types() {
        let ast = r#"{"type":"module","body":[
          {"type":"assign","target":{"type":"name","id":"text"},
           "value":{"type":"string","value":"abc"}},
          {"type":"assign","target":{"type":"name","id":"seen"},
           "value":{"type":"string","value":""}},
          {"type":"for","target":{"type":"name","id":"ch"},
           "iter":{"type":"name","id":"text"},
           "body":[{"type":"assign","target":{"type":"name","id":"seen"},
            "value":{"type":"binary","left":{"type":"name","id":"seen"},
             "operator":"add","right":{"type":"name","id":"ch"}}}]},
          {"type":"assign","target":{"type":"name","id":"scores"},
           "value":{"type":"dict","keys":[{"type":"string","value":"a"}],
            "values":[{"type":"int","value":7}]}},
          {"type":"assign","target":{"type":"name","id":"count"},
           "value":{"type":"int","value":0}},
          {"type":"for","target":{"type":"name","id":"key"},
           "iter":{"type":"name","id":"scores"},
           "body":[{"type":"assign","target":{"type":"name","id":"count"},
            "value":{"type":"binary","left":{"type":"name","id":"count"},
             "operator":"add","right":{"type":"int","value":1}}}]},
          {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
           "args":[{"type":"name","id":"seen"},{"type":"name","id":"count"}],"keywords":[]}}
        ]}"#;
        let code = compile(ast).expect("string and dictionary loops should lower");
        assert_contains_all(
            &code,
            &[
                ".chars().map(|ch| ch.to_string())",
                ".keys().cloned()",
                "println!(\"{} {}\",",
            ],
        );
    }

    #[test]
    fn huge_integer_range_fails_with_actionable_native_diagnostic() {
        let ast = r#"{"type":"module","body":[
          {"type":"for","target":{"type":"name","id":"i"},
           "iter":{"type":"call","function":{"type":"name","id":"range"},
            "args":[{"type":"big_int","value":"2000000000000000000000000"}],"keywords":[]},
           "body":[]}
        ]}"#;
        let error = compile(ast).expect_err("huge native ranges must not compile blindly");
        assert!(error.contains("range() bounds above i64"));
    }

    /// Assert that `compiled` contains all expected substrings.
    fn assert_contains_all(compiled: &str, expected: &[&str]) {
        for s in expected {
            assert!(
                compiled.contains(s),
                "Expected {:?} in generated code:\n---\n{}\n---",
                s,
                compiled
            );
        }
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // MVP: Simple assignment + print
    // Python: x = 10; y = 20; z = x + y; print(z)
    // After copy propagation + constant folding: z = 30_i64
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn mvp_simple_add_and_print() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"assign","target":{"type":"name","id":"x"},"value":{"type":"int","value":10}},
                {"type":"assign","target":{"type":"name","id":"y"},"value":{"type":"int","value":20}},
                {"type":"assign","target":{"type":"name","id":"z"},"value":{
                    "type":"binary","left":{"type":"name","id":"x"},
                    "operator":"add","right":{"type":"name","id":"y"}
                }},
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                    "args":[{"type":"name","id":"z"}]}}
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");

        // Should fold through copy propagation: 30_i64
        assert_contains_all(&code, &["fn main()", "println!(\"{}\",", "30_i64"]);
        assert!(
            !code.contains("{:?}"),
            "must not use debug format:\n{}",
            code
        );
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // For loop accumulator with closed-form reduction
    // Python: total = 0; for i in range(10): total += i; print(total)
    // After loop induction + copy propagation: total = 45_i64
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn for_loop_accumulator() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"assign","target":{"type":"name","id":"total"},"value":{"type":"int","value":0}},
                {"type":"for",
                 "target":{"type":"name","id":"i"},
                 "iter":{"type":"call","function":{"type":"name","id":"range"},"args":[{"type":"int","value":10}]},
                 "body":[
                   {"type":"assign","target":{"type":"name","id":"total"},"value":{
                       "type":"binary","left":{"type":"name","id":"total"},
                       "operator":"add","right":{"type":"name","id":"i"}
                   }}
                 ]
                },
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                    "args":[{"type":"name","id":"total"}]}}
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");
        // Sum of 0..9 is 45. Should fold directly or emit (total + 45)
        assert!(
            code.contains("45_i64") || code.contains("(total + 45_i64)"),
            "folded loop sum not found:\n{}",
            code
        );
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // If / else
    // Python: x = 5; if x > 3: print(x) else: print(0)
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn if_else_branch() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"assign","target":{"type":"name","id":"x"},"value":{"type":"int","value":5}},
                {"type":"if",
                 "test":{"type":"compare","left":{"type":"name","id":"x"},
                         "operators":["gt"],"comparators":[{"type":"int","value":3}]},
                 "body":[{"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                           "args":[{"type":"name","id":"x"}]}}],
                 "orelse":[{"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                             "args":[{"type":"int","value":0}]}}]
                }
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");
        // Dead branch should be eliminated since 5 > 3 is true
        assert_contains_all(&code, &["5_i64"]);
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Typed function definition and call
    // Python: def add(a: int, b: int) -> int: return a + b; print(add(5, 7))
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    /// A zero divisor must reach the `except` handler instead of aborting.
    ///
    /// Division used to be lowered to `.checked_div(..).expect("ZeroDivisionError")`
    /// and `panic!("ZeroDivisionError")`. Both kill the process, so an enclosing
    /// handler never ran and `except ZeroDivisionError` was unreachable. Float
    /// `/` was worse: it emitted a bare `a / b`, so a zero divisor produced
    /// `inf` and the program carried on with a wrong number.
    #[test]
    fn a_zero_divisor_reaches_the_handler_instead_of_aborting() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"funcdef","name":"safe",
                 "args":["a","b"],
                 "arg_annotations":["int","int"],
                 "returns":"int",
                 "body":[
                    {"type":"try",
                     "body":[
                       {"type":"return","value":
                        {"type":"binary","left":{"type":"name","id":"a"},
                         "operator":"floordiv","right":{"type":"name","id":"b"}}}
                     ],
                     "handlers":[
                       {"exc_type":{"type":"name","id":"ZeroDivisionError"},
                        "name":null,
                        "body":[{"type":"return","value":{"type":"int","value":-1}}]}
                     ],
                     "orelse":[],
                     "finalbody":[]}
                 ]}
            ]
        }"#;

        let code = compile(ast).expect("a guarded division should lower");
        assert!(
            code.contains("__tarvos_floor_div_i64"),
            "the division must go through the checking helper:\n{code}"
        );
        assert!(
            !code.contains("panic!(\"ZeroDivisionError"),
            "the guarded path must not abort the process:\n{code}"
        );
        assert!(
            code.contains("__tarvos_error1 = Some(__tarvos_e)"),
            "the failure must be handed to the enclosing try:\n{code}"
        );
    }

    /// A `return` inside an `except` handler must not be lowered to a `break`
    /// aimed at the `try`'s own label.
    ///
    /// The labelled block that represents the `try` body is already closed by the
    /// time a handler runs, so emitting `break '__tarvos_try1;` there produced
    /// `error[E0426]: use of undeclared label` and the generated Rust did not
    /// compile at all. The handler now sees the enclosing `try` — or none — so
    /// its `return` becomes a real return.
    #[test]
    fn a_return_inside_an_except_handler_does_not_target_the_try_label() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"funcdef","name":"pick",
                 "args":["n"],
                 "arg_annotations":["int"],
                 "returns":"int",
                 "body":[
                    {"type":"try",
                     "body":[
                       {"type":"return","value":{"type":"int","value":100}}
                     ],
                     "handlers":[
                       {"exc_type":{"type":"name","id":"ValueError"},
                        "name":null,
                        "body":[{"type":"return","value":{"type":"int","value":-1}}]}
                     ],
                     "orelse":[],
                     "finalbody":[]}
                 ]},
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                 "args":[{"type":"call","function":{"type":"name","id":"pick"},
                          "args":[{"type":"int","value":5}]}]}}
            ]
        }"#;

        let code = compile(ast).expect("a return inside a handler should lower");
        // Exactly one `break '__tarvos_try1`, and it is the one in the try body
        // where the label is still in scope. A second one, emitted for the
        // handler, is what produced `error[E0426]`.
        assert_eq!(
            code.matches("break '__tarvos_try1").count(),
            1,
            "only the try body may break to its own label:\
             \n{code}"
        );
        assert!(
            code.contains("return -1_i64"),
            "the handler's return should be a real return:\
             \n{code}"
        );
    }

    #[test]
    fn typed_function_def_and_call() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"funcdef","name":"add",
                 "args":["a","b"],
                 "arg_annotations":["int","int"],
                 "returns":"int",
                 "body":[
                   {"type":"return","value":{
                       "type":"binary","left":{"type":"name","id":"a"},
                       "operator":"add","right":{"type":"name","id":"b"}
                   }}
                 ]
                },
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                 "args":[{"type":"call","function":{"type":"name","id":"add"},
                          "args":[{"type":"int","value":5},{"type":"int","value":7}]}]}}
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");
        assert_contains_all(
            &code,
            &[
                "fn add(a: i64, b: i64) -> i64",
                "return (a + b)",
                "println!(\"{}\",",
                "add(5_i64, 7_i64)",
            ],
        );
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // String print â€” must NOT have surrounding quotes in output
    // Python: print("hello")
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn print_string_uses_display_not_debug() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                    "args":[{"type":"string","value":"hello"}]}}
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");
        assert!(
            code.contains("println!(\"{}\","),
            "expected display format:\n{}",
            code
        );
        assert!(
            !code.contains("{:?}"),
            "must not use debug format:\n{}",
            code
        );
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Boolean print â€” must use Python capitalisation
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn print_bool_python_capitalisation() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                    "args":[{"type":"bool","value":true}]}},
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                    "args":[{"type":"bool","value":false}]}}
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");
        assert!(code.contains("\"True\""), "True not found:\n{}", code);
        assert!(code.contains("\"False\""), "False not found:\n{}", code);
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Multi-argument print: print("Sum:", 42, True)
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn multi_arg_print_format() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                    "args":[
                        {"type":"string","value":"Sum:"},
                        {"type":"int","value":42},
                        {"type":"bool","value":true}
                    ]}}
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");
        assert_contains_all(
            &code,
            &[
                "println!(\"{} {} {}\",",
                "\"Sum:\".to_string()",
                "42_i64",
                "\"True\"",
            ],
        );
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Constant folding â€” binary ops on literals should be pre-computed
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn constant_folding_eliminates_binary_op() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                    "args":[{"type":"binary","left":{"type":"int","value":10},
                             "operator":"add","right":{"type":"int","value":20}}]}}
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");
        assert!(code.contains("30_i64"), "constant not folded:\n{}", code);
        assert!(
            !code.contains("10_i64 + 20_i64"),
            "binary op not folded:\n{}",
            code
        );
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // None expression must produce a clear error
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn none_expression_produces_clear_error() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"assign","target":{"type":"name","id":"x"},"value":{"type":"none"}}
            ]
        }"#;

        let result = compile(ast);
        assert!(
            result.is_err(),
            "expected error for None, got: {:?}",
            result
        );
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // While loop
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn while_loop_generates_correctly() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"assign","target":{"type":"name","id":"i"},"value":{"type":"int","value":0}},
                {"type":"while",
                 "test":{"type":"compare","left":{"type":"name","id":"i"},
                         "operators":["lt"],"comparators":[{"type":"int","value":5}]},
                 "body":[
                   {"type":"assign","target":{"type":"name","id":"i"},"value":{
                       "type":"binary","left":{"type":"name","id":"i"},
                       "operator":"add","right":{"type":"int","value":1}
                   }}
                 ]
                },
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                    "args":[{"type":"name","id":"i"}]}}
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");
        assert_contains_all(&code, &["while (i < 5_i64)", "i = (i + 1_i64)"]);
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Dead code elimination â€” unused pure let bindings are removed
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn dead_code_unused_variable_eliminated() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"assign","target":{"type":"name","id":"unused"},"value":{"type":"int","value":99}},
                {"type":"assign","target":{"type":"name","id":"keep"},"value":{"type":"int","value":42}},
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                    "args":[{"type":"name","id":"keep"}]}}
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");
        assert!(code.contains("42_i64"), "keep not found:\n{}", code);
        assert!(
            !code.contains("let mut unused"),
            "unused variable not eliminated:\n{}",
            code
        );
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // Float arithmetic
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn float_arithmetic() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"assign","target":{"type":"name","id":"x"},"value":{"type":"float","value":1.5}},
                {"type":"assign","target":{"type":"name","id":"y"},"value":{"type":"float","value":2.5}},
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                    "args":[{"type":"binary","left":{"type":"name","id":"x"},
                             "operator":"add","right":{"type":"name","id":"y"}}]}}
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");
        assert!(
            code.contains("4.0_f64") || code.contains("(x + y)"),
            "float result not found:\n{}",
            code
        );
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // List subscript read
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn list_subscript_read() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"assign","target":{"type":"name","id":"nums"},"value":{
                    "type":"list","elements":[
                        {"type":"int","value":10},
                        {"type":"int","value":20},
                        {"type":"int","value":30}
                    ]
                }},
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                    "args":[{"type":"subscript","value":{"type":"name","id":"nums"},"index":{"type":"int","value":1}}]}}
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");
        // Sequence reads resolve the index through the runtime so a negative
        // index counts from the end and an out-of-range read raises IndexError,
        // matching CPython. `nums[1 as usize]` could express neither rule.
        assert_contains_all(
            &code,
            &[
                "vec![10_i64, 20_i64, 30_i64]",
                "fn __tarvos_index(length: i64, index: i64) -> usize",
                "nums[__tarvos_index(nums.len() as i64, (1_i64) as i64)]",
            ],
        );
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // List subscript in-place mutation: arr[0] = 99
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    #[test]
    fn list_subscript_mutation() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type":"assign","target":{"type":"name","id":"nums"},"value":{
                    "type":"list","elements":[{"type":"int","value":10}]
                }},
                {"type":"assign",
                 "target":{"type":"subscript","value":{"type":"name","id":"nums"},"index":{"type":"int","value":0}},
                 "value":{"type":"int","value":99}},
                {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},
                    "args":[{"type":"subscript","value":{"type":"name","id":"nums"},"index":{"type":"int","value":0}}]}}
            ]
        }"#;

        let code = compile(ast).expect("pipeline should succeed");
        // The index is resolved into a temporary first: inlining `nums.len()`
        // into the subscript would borrow the vector immutably while the store
        // borrows it mutably.
        assert_contains_all(
            &code,
            &[
                "let __tarvos_index_0 = __tarvos_index(nums.len() as i64, (0_i64) as i64);",
                "nums[__tarvos_index_0] = 99_i64;",
            ],
        );
    }

    #[test]
    fn bitwise_shift_and_floor_division_lower_to_native_rust() {
        // A function parameter keeps the values unknown, so no constant folding hides
        // the emitted operators.
        let ast = r#"{"type":"module","body":[
          {"type":"funcdef","name":"kernel","args":["seed"],"arg_annotations":["int"],"returns":null,"body":[
            {"type":"assign","target":{"type":"name","id":"state"},"value":{"type":"name","id":"seed"}},
            {"type":"assign","target":{"type":"name","id":"state"},"value":{"type":"binary",
              "left":{"type":"name","id":"state"},"operator":"bitxor","right":{"type":"int","value":5}}},
            {"type":"assign","target":{"type":"name","id":"state"},"value":{"type":"binary",
              "left":{"type":"name","id":"state"},"operator":"bitand","right":{"type":"int","value":4294967295}}},
            {"type":"assign","target":{"type":"name","id":"merged"},"value":{"type":"binary",
              "left":{"type":"name","id":"state"},"operator":"bitor","right":{"type":"int","value":1}}},
            {"type":"assign","target":{"type":"name","id":"shifted"},"value":{"type":"binary",
              "left":{"type":"name","id":"merged"},"operator":"lshift","right":{"type":"int","value":3}}},
            {"type":"assign","target":{"type":"name","id":"restored"},"value":{"type":"binary",
              "left":{"type":"name","id":"shifted"},"operator":"rshift","right":{"type":"int","value":3}}},
            {"type":"assign","target":{"type":"name","id":"floored"},"value":{"type":"binary",
              "left":{"type":"name","id":"restored"},"operator":"floordiv","right":{"type":"int","value":7}}},
            {"type":"assign","target":{"type":"name","id":"negative"},"value":{"type":"unary",
              "operator":"usub","operand":{"type":"name","id":"floored"}}},
            {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},"args":[
              {"type":"name","id":"negative"}]}}
          ]}
        ]}"#;

        let code = compile(ast).expect("bitwise subset should lower");
        assert_contains_all(
            &code,
            &[
                " ^ ",
                " & ",
                " | ",
                "checked_shl",
                "checked_shr",
                "checked_div",
                "-(",
            ],
        );
    }

    #[test]
    fn floor_division_keeps_python_rounding_towards_negative_infinity() {
        // Python: -7 // 2 == -4, but Rust's `/` truncates to -3.
        let ast = r#"{"type":"module","body":[
          {"type":"funcdef","name":"kernel","args":["seed"],"arg_annotations":["int"],"returns":null,"body":[
            {"type":"assign","target":{"type":"name","id":"value"},"value":{"type":"unary",
              "operator":"usub","operand":{"type":"name","id":"seed"}}},
            {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},"args":[
              {"type":"binary","left":{"type":"name","id":"value"},
               "operator":"floordiv","right":{"type":"int","value":2}}]}}
          ]}
        ]}"#;

        let code = compile(ast).expect("floor division should lower");
        assert!(
            code.contains("__tarvos_remainder < 0_i64")
                && code.contains("__tarvos_quotient - 1_i64"),
            "floor division must correct the truncated quotient:\n{code}"
        );
    }

    #[test]
    fn unsupported_shift_of_invalid_type_is_rejected() {
        // Bitwise operators are integer-only; floats must not silently truncate.
        let ast = r#"{"type":"module","body":[
          {"type":"assign","target":{"type":"name","id":"value"},"value":{"type":"binary",
            "left":{"type":"float","value":1.5},"operator":"bitand","right":{"type":"int","value":3}}}
        ]}"#;

        let error = compile(ast).expect_err("float bitwise operands must be rejected");
        assert!(
            error.contains("unsupported operation"),
            "unexpected diagnostic for float bitwise operands: {error}"
        );
    }

    #[test]
    fn comma_grouping_format_spec_uses_the_grouping_helper() {
        let ast = r#"{"type":"module","body":[
          {"type":"funcdef","name":"report","args":["total"],"arg_annotations":["int"],"returns":null,"body":[
            {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},"args":[
              {"type":"format_string","parts":[
                {"type":"literal","value":"total = "},
                {"type":"value","value":{"type":"name","id":"total"},"format_spec":",","conversion":null}
              ]}]}}
          ]}
        ]}"#;

        let code = compile(ast).expect("grouping spec should lower");
        assert!(
            code.contains("fn __tarvos_group_numeric")
                && code.contains("__tarvos_group_numeric(total, ',')"),
            "comma grouping should route through the generated helper:\n{code}"
        );
    }

    #[test]
    fn nested_subscript_assignment_keeps_each_index_distinct() {
        let ast = r#"{"type":"module","body":[
          {"type":"funcdef","name":"bump","args":["grid","row","column","delta"],"arg_annotations":["int","int","int","int"],"returns":null,"body":[
            {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},"args":[
              {"type":"binary",
               "left":{"type":"subscript","value":{"type":"subscript","value":{"type":"name","id":"grid"},"index":{"type":"name","id":"row"}},"index":{"type":"name","id":"column"}},
               "operator":"add","right":{"type":"name","id":"delta"}}]}}
          ]}
        ]}"#;

        let code = compile(ast).expect("nested reads should lower");
        assert_contains_all(&code, &["grid[(", "row", "column", "delta"]);
    }

    #[test]
    fn matrix_like_nested_index_assignment_emits_a_chained_store() {
        let ast = r#"{"type":"module","body":[
          {"type":"assign","target":{"type":"name","id":"grid"},"value":{"type":"list","elements":[]}},
          {"type":"assign",
           "target":{"type":"subscript",
             "value":{"type":"subscript","value":{"type":"name","id":"grid"},"index":{"type":"int","value":0}},
             "index":{"type":"int","value":1}},
           "value":{"type":"binary","operator":"add",
             "left":{"type":"subscript",
               "value":{"type":"subscript","value":{"type":"name","id":"grid"},"index":{"type":"int","value":0}},
               "index":{"type":"int","value":1}},
             "right":{"type":"int","value":2}}}
        ]}"#;

        let code = compile(ast).expect("nested subscript assignment should lower");
        // Both indices are resolved into named temporaries before the store, so
        // the chain is `grid[<tmp0>][<tmp1>]`.
        assert!(
            code.contains("let __tarvos_index_0 =")
                && code.contains("let __tarvos_index_1 =")
                && code.contains("grid[__tarvos_index_0][__tarvos_index_1] ="),
            "nested assignment must chain both resolved indices:\n{code}"
        );
    }

    #[test]
    fn nested_list_comprehension_keeps_inner_and_outer_types() {
        let ast = r#"{"type":"module","body":[
          {"type":"assign","target":{"type":"name","id":"grid"},"value":{"type":"list_comp",
            "elt":{"type":"list_comp","elt":{"type":"name","id":"j"},"target":"j",
                   "iter":{"type":"call","function":{"type":"name","id":"range"},"args":[{"type":"name","id":"n"}],"keywords":[]},
                   "condition":null},
            "target":"i",
            "iter":{"type":"call","function":{"type":"name","id":"range"},"args":[{"type":"name","id":"n"}],"keywords":[]},
            "condition":null}},
          {"type":"expr","value":{"type":"call","function":{"type":"name","id":"print"},"args":[
            {"type":"name","id":"grid"}]}}
        ]}"#;

        let code = compile(ast).expect("nested comprehensions should lower");
        assert!(
            code.contains(".into_iter().map(|i|") && code.contains(".into_iter().map(|j|"),
            "outer and inner loops must both be emitted:\n{code}"
        );
    }

    #[test]
    fn dictionary_through_path_assignment_requests_the_fallback() {
        let ast = r#"{"type":"module","body":[
          {"type":"assign","target":{"type":"name","id":"store"},"value":{"type":"dict",
            "keys":[{"type":"string","value":"row"}],"values":[{"type":"dict",
              "keys":[{"type":"string","value":"cell"}],"values":[{"type":"int","value":1}]}]}},
          {"type":"assign",
           "target":{"type":"subscript",
             "value":{"type":"subscript","value":{"type":"name","id":"store"},"index":{"type":"string","value":"row"}},
             "index":{"type":"string","value":"cell"}},
           "value":{"type":"int","value":2}}
        ]}"#;

        let error = compile(ast).expect_err("dict-in-dict write must not be guessed native");
        assert!(
            error.contains("dictionary"),
            "unexpected diagnostic for nested dict write: {error}"
        );
    }

    #[test]
    fn unsupported_lambda_fails_fast() {
        let ast = r#"{
            "type": "module",
            "body": [
                {"type": "expr", "value": {
                    "type": "lambda",
                    "args": [],
                    "body": {"type": "int", "value": 1}
                }}
            ]
        }"#;

        let result = compile(ast);
        assert!(
            result.is_err(),
            "unsupported lambda should fail fast, got: {:?}",
            result
        );
        let message = result.unwrap_err();
        let lower = message.to_lowercase();
        assert!(
            lower.contains("unsupported")
                || lower.contains("lambda")
                || lower.contains("deserialize")
                || lower.contains("unknown"),
            "expected clear unsupported-feature rejection, got: {}",
            message
        );
    }
}
