// tarvos-tests: Integration tests for the full compiler pipeline.

/// Re-export nothing — this crate is test-only.
pub fn _placeholder() {}

#[cfg(test)]
mod pipeline {
    use tarvos_analysis::lower_module;
    use tarvos_codegen_rust::RustCodegen;
    use tarvos_optimizer::Optimizer;
    use tarvos_parser::parse_python_ast;

    // ────────────────────────────────────────────────────────────────────────
    // Helpers
    // ────────────────────────────────────────────────────────────────────────

    /// Run the full pipeline: JSON AST → Rust source string.
    fn compile(ast_json: &str) -> Result<String, String> {
        let module = parse_python_ast(ast_json).map_err(|e| e.to_string())?;
        let ir = lower_module(&module).map_err(|e| e.to_string())?;
        let ir = Optimizer::optimize(&ir).map_err(|e| e.to_string())?;
        RustCodegen::generate(&ir).map_err(|e| e.to_string())
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

    // ────────────────────────────────────────────────────────────────────────
    // MVP: Simple assignment + print
    // Python: x = 10; y = 20; z = x + y; print(z)
    // After copy propagation + constant folding: z = 30_i64
    // ────────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────────
    // For loop accumulator with closed-form reduction
    // Python: total = 0; for i in range(10): total += i; print(total)
    // After loop induction + copy propagation: total = 45_i64
    // ────────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────────
    // If / else
    // Python: x = 5; if x > 3: print(x) else: print(0)
    // ────────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────────
    // Typed function definition and call
    // Python: def add(a: int, b: int) -> int: return a + b; print(add(5, 7))
    // ────────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────────
    // String print — must NOT have surrounding quotes in output
    // Python: print("hello")
    // ────────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────────
    // Boolean print — must use Python capitalisation
    // ────────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────────
    // Multi-argument print: print("Sum:", 42, True)
    // ────────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────────
    // Constant folding — binary ops on literals should be pre-computed
    // ────────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────────
    // None expression must produce a clear error
    // ────────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────────
    // While loop
    // ────────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────────
    // Dead code elimination — unused pure let bindings are removed
    // ────────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────────
    // Float arithmetic
    // ────────────────────────────────────────────────────────────────────────
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

    // ────────────────────────────────────────────────────────────────────────
    // List subscript read
    // ────────────────────────────────────────────────────────────────────────
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
        assert_contains_all(
            &code,
            &["vec![10_i64, 20_i64, 30_i64]", "nums[(1_i64 as usize)]"],
        );
    }

    // ────────────────────────────────────────────────────────────────────────
    // List subscript in-place mutation: arr[0] = 99
    // ────────────────────────────────────────────────────────────────────────
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
        assert_contains_all(&code, &["nums[(0_i64 as usize)] = 99_i64;"]);
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
