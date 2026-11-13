import ast
import json
import sys


def convert_annotation(node):
    if node is None:
        return None
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        base = convert_annotation(node.value)
        return f"{base}.{node.attr}" if base else node.attr
    if isinstance(node, ast.Subscript):
        value = convert_annotation(node.value)
        slice_value = convert_annotation(node.slice)
        return f"{value}[{slice_value}]" if value and slice_value else value
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value
    if isinstance(node, ast.Call):
        func_name = convert_annotation(node.func)
        return func_name
    return None


class PythonAstExporter(ast.NodeVisitor):
    def __init__(self):
        self.numpy_aliases = set()

    @staticmethod
    def location(node):
        line = getattr(node, "lineno", None)
        column = getattr(node, "col_offset", None)
        if line is None:
            return ""
        return f" at line {line}, column {(column or 0) + 1}"

    def visit_Module(self, node):
        self.numpy_aliases = {
            alias.asname or alias.name.split(".")[0]
            for statement in node.body
            if isinstance(statement, ast.Import)
            for alias in statement.names
            if alias.name == "numpy"
        }
        return {"type": "module", "body": [self.visit(stmt) for stmt in node.body]}

    def visit_Import(self, node):
        return {
            "type": "import",
            "names": [{"name": alias.name, "asname": alias.asname} for alias in node.names],
        }

    def visit_ImportFrom(self, node):
        if node.module is None:
            raise ValueError(f"relative imports are not supported{self.location(node)}")
        return {
            "type": "import_from",
            "module": node.module,
            "names": [{"name": alias.name, "asname": alias.asname} for alias in node.names],
        }

    def visit_Assign(self, node):
        if len(node.targets) != 1:
            raise ValueError(
                f"multiple assignment targets are not supported{self.location(node)}"
            )
        return {"type": "assign", "target": self.visit(node.targets[0]), "value": self.visit(node.value)}

    def visit_AnnAssign(self, node):
        target = self.visit(node.target)
        value = self.visit(node.value) if node.value is not None else None
        return {
            "type": "ann_assign",
            "target": target,
            "annotation": convert_annotation(node.annotation),
            "value": value,
        }

    def visit_AugAssign(self, node):
        operators = {
            ast.Add: "add",
            ast.Sub: "sub",
            ast.Mult: "mul",
            ast.Div: "div",
            ast.FloorDiv: "floordiv",
            ast.Mod: "mod",
            ast.Pow: "pow",
            ast.BitAnd: "bitand",
            ast.BitOr: "bitor",
            ast.BitXor: "bitxor",
            ast.LShift: "lshift",
            ast.RShift: "rshift",
        }
        op_type = type(node.op)
        if op_type not in operators:
            raise ValueError(
                f"unsupported augmented assignment operator: {op_type.__name__}"
                f"{self.location(node)}"
            )
        return {
            "type": "assign",
            "target": self.visit(node.target),
            "value": {"type": "binary", "left": self.visit(node.target), "operator": operators[op_type], "right": self.visit(node.value)},
        }

    def visit_Assert(self, node):
        return {
            "type": "assert",
            "test": self.visit(node.test),
            "msg": self.visit(node.msg) if node.msg is not None else None,
        }

    def visit_Delete(self, node):
        return {"type": "delete", "targets": [self.visit(target) for target in node.targets]}

    def visit_Global(self, node):
        return {"type": "global", "names": list(node.names)}

    def visit_Nonlocal(self, node):
        return {"type": "nonlocal", "names": list(node.names)}

    def visit_Expr(self, node):
        return {"type": "expr", "value": self.visit(node.value)}

    def visit_ClassDef(self, node):
        return {
            "type": "classdef",
            "name": node.name,
            "bases": [convert_annotation(base) for base in node.bases],
            "body": [self.visit(statement) for statement in node.body],
        }

    def visit_Name(self, node):
        return {"type": "name", "id": node.id}

    def visit_Attribute(self, node):
        return {
            "type": "attribute",
            "value": self.visit(node.value),
            "attr": node.attr,
        }

    def visit_Constant(self, node):
        value = node.value
        if isinstance(value, bool):
            return {"type": "bool", "value": value}
        if isinstance(value, int):
            if -(2**63) <= value <= 2**63 - 1:
                return {"type": "int", "value": value}
            return {"type": "big_int", "value": str(value)}
        if isinstance(value, float):
            return {"type": "float", "value": value}
        if isinstance(value, str):
            return {"type": "string", "value": value}
        if value is None:
            return {"type": "none"}
        raise ValueError(f"unsupported constant: {type(value).__name__}")

    def visit_BinOp(self, node):
        operators = {
            ast.Add: "add",
            ast.Sub: "sub",
            ast.Mult: "mul",
            ast.Div: "div",
            ast.FloorDiv: "floordiv",
            ast.Mod: "mod",
            ast.Pow: "pow",
            ast.BitAnd: "bitand",
            ast.BitOr: "bitor",
            ast.BitXor: "bitxor",
            ast.LShift: "lshift",
            ast.RShift: "rshift",
        }
        op_type = type(node.op)
        if op_type not in operators:
            raise ValueError(
                f"unsupported operator: {op_type.__name__}{self.location(node)}"
            )
        return {"type": "binary", "left": self.visit(node.left), "operator": operators[op_type], "right": self.visit(node.right)}

    def visit_Call(self, node):
        if isinstance(node.func, ast.Attribute):
            if (
                isinstance(node.func.value, ast.Name)
                and node.func.value.id in self.numpy_aliases
                and node.func.attr in {"arange", "zeros", "ones"}
                and len(node.args) == 1
                and isinstance(node.args[0], ast.Constant)
                and isinstance(node.args[0].value, int)
                and node.args[0].value >= 0
            ):
                size = node.args[0].value
                if node.func.attr == "arange":
                    elements = [{"type": "int", "value": value} for value in range(size)]
                else:
                    elements = [{"type": "int", "value": 0} for _ in range(size)]
                return {"type": "list", "elements": elements}
            return {
                "type": "method_call",
                "object": self.visit(node.func.value),
                "method": node.func.attr,
                "args": [self.visit(arg) for arg in node.args],
            }
        return {"type": "call", "function": self.visit(node.func), "args": [self.visit(arg) for arg in node.args], "keywords": [self.visit(keyword) for keyword in node.keywords]}

    def visit_keyword(self, node):
        return {"type": "keyword", "arg": node.arg, "value": self.visit(node.value)}

    def visit_Compare(self, node):
        operators = {ast.Eq: "eq", ast.NotEq: "ne", ast.Lt: "lt", ast.LtE: "le", ast.Gt: "gt", ast.GtE: "ge"}
        ops = []
        for op in node.ops:
            op_type = type(op)
            if op_type not in operators:
                raise ValueError(
                    f"unsupported comparison: {op_type.__name__}{self.location(node)}"
                )
            ops.append(operators[op_type])
        return {"type": "compare", "left": self.visit(node.left), "operators": ops, "comparators": [self.visit(c) for c in node.comparators]}

    def visit_If(self, node):
        return {"type": "if", "test": self.visit(node.test), "body": [self.visit(x) for x in node.body], "orelse": [self.visit(x) for x in node.orelse]}

    def visit_While(self, node):
        return {"type": "while", "test": self.visit(node.test), "body": [self.visit(x) for x in node.body]}

    def visit_For(self, node):
        return {"type": "for", "target": self.visit(node.target), "iter": self.visit(node.iter), "body": [self.visit(x) for x in node.body]}

    def visit_FunctionDef(self, node):
        arg_annotations = [convert_annotation(arg.annotation) for arg in node.args.args]
        return {
            "type": "funcdef",
            "name": node.name,
            "args": [arg.arg for arg in node.args.args],
            "arg_annotations": arg_annotations,
            "body": [self.visit(x) for x in node.body],
            "returns": convert_annotation(node.returns),
        }

    def visit_Return(self, node):
        return {"type": "return", "value": self.visit(node.value) if node.value is not None else None}

    def visit_Break(self, node):
        return {"type": "break"}

    def visit_Continue(self, node):
        return {"type": "continue"}

    def visit_Raise(self, node):
        return {"type": "raise", "exc": self.visit(node.exc) if node.exc is not None else None}

    def visit_Try(self, node):
        handlers = []
        for handler in node.handlers:
            handlers.append({
                "name": handler.name,
                "exc_type": self.visit(handler.type) if handler.type is not None else None,
                "body": [self.visit(x) for x in handler.body],
            })
        return {
            "type": "try",
            "body": [self.visit(x) for x in node.body],
            "handlers": handlers,
            "orelse": [self.visit(x) for x in node.orelse],
            "finalbody": [self.visit(x) for x in node.finalbody],
        }

    def visit_With(self, node):
        items = []
        for item in node.items:
            items.append({
                "context_expr": self.visit(item.context_expr),
                "optional_vars": self.visit(item.optional_vars) if item.optional_vars is not None else None,
            })
        return {
            "type": "with",
            "items": items,
            "body": [self.visit(x) for x in node.body],
        }

    def visit_Slice(self, node):
        return {
            "type": "slice",
            "lower": self.visit(node.lower) if node.lower is not None else None,
            "upper": self.visit(node.upper) if node.upper is not None else None,
            "step": self.visit(node.step) if node.step is not None else None,
        }

    def visit_UnaryOp(self, node):
        operators = {ast.USub: "usub", ast.UAdd: "uadd", ast.Not: "not", ast.Invert: "invert"}
        op_type = type(node.op)
        if op_type not in operators:
            raise ValueError(f"unsupported unary operator: {op_type.__name__}{self.location(node)}")
        return {
            "type": "unary",
            "operator": operators[op_type],
            "operand": self.visit(node.operand),
        }

    def visit_BoolOp(self, node):
        operator = "and" if isinstance(node.op, ast.And) else "or"
        return {
            "type": "bool_op",
            "operator": operator,
            "values": [self.visit(value) for value in node.values],
        }

    def visit_IfExp(self, node):
        return {
            "type": "if_exp",
            "test": self.visit(node.test),
            "body": self.visit(node.body),
            "orelse": self.visit(node.orelse),
        }

    def visit_Lambda(self, node):
        return {
            "type": "lambda",
            "args": [arg.arg for arg in node.args.args],
            "body": self.visit(node.body),
        }

    def visit_List(self, node):
        return {"type": "list", "elements": [self.visit(x) for x in node.elts]}

    def visit_ListComp(self, node):
        if len(node.generators) != 1 or node.generators[0].is_async:
            raise ValueError(
                "only one synchronous list-comprehension generator is supported"
                f"{self.location(node)}"
            )
        generator = node.generators[0]
        if not isinstance(generator.target, ast.Name):
            raise ValueError(
                "list-comprehension targets must be simple names"
                f"{self.location(generator.target)}"
            )
        if len(generator.ifs) > 1:
            raise ValueError(
                "list comprehensions support at most one filter"
                f"{self.location(node)}"
            )
        return {
            "type": "list_comp",
            "elt": self.visit(node.elt),
            "target": generator.target.id,
            "iter": self.visit(generator.iter),
            "condition": self.visit(generator.ifs[0]) if generator.ifs else None,
        }

    def visit_Set(self, node):
        return {"type": "set", "elements": [self.visit(x) for x in node.elts]}

    def visit_Starred(self, node):
        return {"type": "starred", "value": self.visit(node.value)}

    def visit_Tuple(self, node):
        return {"type": "tuple", "elements": [self.visit(x) for x in node.elts]}

    def visit_Dict(self, node):
        if any(key is None for key in node.keys):
            raise ValueError(
                "dictionary unpacking (**mapping) is not supported"
                f"{self.location(node)}"
            )
        return {
            "type": "dict",
            "keys": [self.visit(key) for key in node.keys],
            "values": [self.visit(value) for value in node.values],
        }

    def visit_Subscript(self, node):
        return {"type": "subscript", "value": self.visit(node.value), "index": self.visit(node.slice)}

    def visit_JoinedStr(self, node):
        parts = []
        for part in node.values:
            if isinstance(part, ast.Constant) and isinstance(part.value, str):
                parts.append({"type": "literal", "value": part.value})
            elif isinstance(part, ast.FormattedValue):
                if part.conversion not in (-1, 115, 114, 97):
                    raise ValueError(
                        f"unsupported f-string conversion: {part.conversion}"
                        f"{self.location(part)}"
                    )
                format_spec = None
                if part.format_spec is not None:
                    if not all(
                        isinstance(spec_part, ast.Constant)
                        and isinstance(spec_part.value, str)
                        for spec_part in part.format_spec.values
                    ):
                        raise ValueError(
                            "dynamic f-string format specifications are not supported"
                            f"{self.location(part)}"
                        )
                    format_spec = "".join(
                        spec_part.value for spec_part in part.format_spec.values
                    )
                parts.append(
                    {
                        "type": "value",
                        "value": self.visit(part.value),
                        "format_spec": format_spec,
                        "conversion": (
                            chr(part.conversion) if part.conversion != -1 else None
                        ),
                    }
                )
            else:
                raise ValueError(
                    f"unsupported f-string part: {type(part).__name__}{self.location(part)}"
                )
        return {"type": "format_string", "parts": parts}

    def visit_AsyncFunctionDef(self, node):
        return {
            "type": "AsyncFunctionDef",
            "name": node.name,
            "args": self.visit(node.args),
            "body": [self.visit(stmt) for stmt in node.body],
            "decorator_list": [self.visit(dec) for dec in node.decorator_list]
        }

    def visit_Await(self, node):
        return {
            "type": "Await",
            "value": self.visit(node.value)
        }



def export_python_ast(source):
    try:
        source = source.lstrip("\ufeff")
        tree = ast.parse(source)
    except SyntaxError as error:
        line = error.lineno or 0
        column = (error.offset or 1)
        raise ValueError(
            f"Python syntax error at line {line}, column {column}: {error.msg}"
        ) from error
    exporter = PythonAstExporter()
    result = exporter.visit(tree)
    print("Native Python AST Visitor re-enabled via ast.NodeVisitor", file=sys.stderr)
    return json.dumps(result, separators=(",", ":"))


if __name__ == "__main__":
    try:
        source = sys.stdin.buffer.read().decode("utf-8")
        print(export_python_ast(source))
    except UnicodeDecodeError as error:
        raise ValueError(
            f"Python source is not valid UTF-8 at byte offset {error.start}"
        ) from error
