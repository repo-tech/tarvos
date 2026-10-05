"""Tarvos Real Python Compatibility / Stress Test.

Ordinary Python on purpose: nothing here is written for Tarvos, and the expected
output is deterministic. Every construct below is compared against CPython, so a
difference in stdout, stderr, or exit status is a real compatibility gap rather
than a difference in what the test happens to exercise.
"""

import math
import statistics


# 1. FUNCTIONS / FUNCTION CALLS


def add(a, b):
    return a + b


def multiply(a, b):
    return a * b


def transform(value, fn):
    return fn(value)


def compose(a, b, value):
    return a(b(value))


def factorial(n):
    if n <= 1:
        return 1
    return n * factorial(n - 1)


def fibonacci(n):
    if n <= 1:
        return n
    return fibonacci(n - 1) + fibonacci(n - 2)


# 2. DYNAMIC VARIABLES

dynamic_value = 10
dynamic_value = "hello"
dynamic_value = [1, 2, 3]
dynamic_value = {"x": 10, "y": 20}
dynamic_value = 42.5

print("dynamic:", dynamic_value)


# 3. INTEGER / FLOAT NUMERICS

numbers = [1, 2, 3, 4, 5]

integer_total = sum(numbers)
float_total = sum(x * 0.5 for x in numbers)

print("integer_total:", integer_total)
print("float_total:", float_total)
print("mean:", statistics.mean(numbers))
print("sqrt:", math.sqrt(144))
print("power:", 2 ** 10)


# 4. LISTS

values = [5, 2, 9, 1, 7, 3]

values.append(11)
values.extend([13, 15])
values.insert(0, -1)

print("list:", values)
print("first:", values[0])
print("last:", values[-1])
print("slice:", values[2:6])
print("reverse_slice:", values[::-1])
print("contains:", 7 in values)

values.remove(2)
removed = values.pop()

print("after_remove:", values)
print("popped:", removed)

values.sort()
print("sorted:", values)

values.reverse()
print("reversed:", values)


# 5. LIST COMPREHENSIONS

squares = [x * x for x in range(10)]
evens = [x for x in range(30) if x % 2 == 0]
nested = [x * y for x in range(3) for y in range(4)]

print("squares:", squares)
print("evens:", evens)
print("nested:", nested)


# 6. STRINGS

text = "Tarvos Python Compiler"

print("lower:", text.lower())
print("upper:", text.upper())
print("strip:", " hello ".strip())
print("split:", text.split())
print("replace:", text.replace("Python", "Rust"))
print("startswith:", text.startswith("Tarvos"))
print("endswith:", text.endswith("Compiler"))
print("find:", text.find("Python"))
print("count:", text.count("o"))
print("join:", "-".join(["Tarvos", "Rust", "Native"]))

name = "Himanshu"
version = 1.5

print("format:", "{} v{}".format(name, version))
print("fstring:", f"{name} v{version}")


# 7. STRING INDEXING / SLICING

s = "abcdefghijklmnopqrstuvwxyz"

print("char0:", s[0])
print("char_last:", s[-1])
print("slice1:", s[0:5])
print("slice2:", s[5:15])
print("reverse:", s[::-1])


# 8. DICTIONARIES

user = {
    "name": "Himanshu",
    "age": 25,
    "role": "developer",
}

user["language"] = "Python"
user["compiler"] = "Tarvos"

print("dict:", user)
print("name:", user["name"])
print("get:", user.get("missing", "default"))
print("keys:", list(user.keys()))
print("values:", list(user.values()))
print("items:", list(user.items()))


# 9. DICT COMPREHENSION

squared_map = {x: x * x for x in range(6)}

print("squared_map:", squared_map)


# 10. TUPLES

point = (10, 20)

x, y = point

print("tuple:", point)
print("unpacked:", x, y)


# 11. SETS

a = {1, 2, 3, 4}
b = {3, 4, 5, 6}

print("set_union:", sorted(a | b))
print("set_intersection:", sorted(a & b))
print("set_difference:", sorted(a - b))
print("set_symmetric:", sorted(a ^ b))
# 12. ENUMERATE / ZIP

names = ["A", "B", "C"]
scores = [10, 20, 30]

for index, name in enumerate(names):
    print("enum:", index, name)

for name, score in zip(names, scores):
    print("zip:", name, score)


# 13. MAP / FILTER

mapped = list(map(lambda x: x * 2, [1, 2, 3, 4]))
filtered = list(filter(lambda x: x % 2 == 0, range(10)))

print("mapped:", mapped)
print("filtered:", filtered)


# 14. ANY / ALL

print("any:", any(x > 8 for x in range(10)))
print("all:", all(x < 20 for x in range(10)))


# 15. SORTING WITH KEY

records = [
    ("Alice", 90),
    ("Bob", 70),
    ("Charlie", 85),
    ("David", 95),
]

sorted_records = sorted(records, key=lambda item: item[1])

print("sorted_records:", sorted_records)


# 16. NESTED DATA

projects = [
    {
        "name": "Tarvos",
        "language": "Rust",
        "stars": 100,
    },
    {
        "name": "Compiler",
        "language": "Python",
        "stars": 50,
    },
    {
        "name": "Runtime",
        "language": "Rust",
        "stars": 75,
    },
]

for project in projects:
    print(
        "project:",
        project["name"],
        project["language"],
        project["stars"],
    )


# 17. FUNCTIONS RETURNING COLLECTIONS

def build_numbers(n):
    result = []

    for i in range(n):
        result.append(i * i)

    return result


print("build_numbers:", build_numbers(10))


# 18. HIGHER-ORDER FUNCTIONS

def apply_operation(values, operation):
    result = []

    for value in values:
        result.append(operation(value))

    return result


def cube(x):
    return x * x * x


print(
    "higher_order:",
    apply_operation([1, 2, 3, 4], cube),
)


# 19. CLOSURE

def make_multiplier(factor):
    def multiply_value(value):
        return value * factor

    return multiply_value


double = make_multiplier(2)
triple = make_multiplier(3)

print("closure:", double(10), triple(10))


# 20. EXCEPTIONS

def safe_divide(a, b):
    try:
        return a / b
    except ZeroDivisionError:
        return None


print("exception_ok:", safe_divide(10, 2))
print("exception_zero:", safe_divide(10, 0))


# 21. NESTED EXCEPTION

def nested_exception():
    try:
        try:
            return 10 / 0
        except ZeroDivisionError:
            return "inner-caught"
        finally:
            pass
    except Exception:
        return "outer-caught"


print("nested_exception:", nested_exception())


# 22. TYPE / NONE BEHAVIOR

value = None

print("none:", value is None)
print("not_none:", value is not None)

value = 100

print("type_int:", type(value).__name__)


# 23. BOOLEAN / CONDITIONAL LOGIC

def classify(value):
    if value is None:
        return "none"

    if value < 0:
        return "negative"

    if value == 0:
        return "zero"

    if value < 10:
        return "small"

    return "large"


for value in [-5, 0, 3, 10, None]:
    print("classify:", classify(value))


# 24. WHILE LOOP

counter = 0
total = 0

while counter < 10:
    total += counter
    counter += 1

print("while_total:", total)


# 25. BREAK / CONTINUE

result = []

for i in range(20):
    if i % 2 == 0:
        continue

    if i > 11:
        break

    result.append(i)

print("break_continue:", result)


# 26. MATRIX / NESTED LISTS

matrix = [
    [1, 2, 3],
    [4, 5, 6],
    [7, 8, 9],
]

diagonal = []

for i in range(len(matrix)):
    diagonal.append(matrix[i][i])

print("diagonal:", diagonal)


# 27. DATA TRANSFORMATION

data = [
    {"name": "a", "value": 10},
    {"name": "b", "value": 20},
    {"name": "c", "value": 30},
]

transformed = [
    {
        "name": item["name"].upper(),
        "value": item["value"] * 2,
    }
    for item in data
]

print("transformed:", transformed)


# 28. STATISTICS

sample = [10, 20, 20, 30, 40, 50]

print("statistics_mean:", statistics.mean(sample))
print("statistics_median:", statistics.median(sample))
print("statistics_mode:", statistics.mode(sample))
print("statistics_variance:", statistics.variance(sample))


# 29. RECURSIVE WORKLOAD

print("factorial:", factorial(8))
print("fibonacci:", fibonacci(12))


# 30. REALISTIC MINI WORKLOAD

def analyze_records(records):
    valid = [
        item
        for item in records
        if item["active"] and item["score"] >= 50
    ]

    scores = [item["score"] for item in valid]

    if not scores:
        return {
            "count": 0,
            "average": 0,
            "maximum": 0,
            "minimum": 0,
        }

    return {
        "count": len(scores),
        "average": statistics.mean(scores),
        "maximum": max(scores),
        "minimum": min(scores),
    }


records = [
    {"name": "A", "score": 80, "active": True},
    {"name": "B", "score": 40, "active": True},
    {"name": "C", "score": 90, "active": True},
    {"name": "D", "score": 30, "active": False},
    {"name": "E", "score": 70, "active": True},
]

print("analysis:", analyze_records(records))


# 31. DETERMINISTIC COMPUTE WORKLOAD

def compute_workload(n):
    total = 0

    for i in range(1, n + 1):
        total += (i * i + i * 3) % 97

    return total


print("compute_workload:", compute_workload(10000))


# 32. FINAL INTEGRITY MARKER

print("TARVOS_STRESS_TEST_COMPLETE")