print(1 + 2)
print(10 - 4)
print(6 * 7)
print(2 + 3 * 4)
print((2 + 3) * 4)
print(-5 + 3)
print(17 % 5)
print(2.5 + 1.5)
print(7.0 / 2)
print(1.0 / 4)
print(3.5 * 2)
print(10 / 5)
print(10 / 4)
print(0.1 + 0.2)
a = 1.5
b = 2.5
print(a + b)
print(a * b)
print(a / b)
i = 3
f = 2.0
print(i + f)
print(i * f)
print(i / f)
print(f + i)
print(f / i)
# `True + True` is valid Python (bool is a subclass of int) but is not part of
# the supported numeric subset, so it is not asserted here.

# Conversions. `int("3")` must parse, not cast: Rust's `as` cannot convert a
# String to i64 at all.
print(int("3"))
print(int(" 42 "))
print(int(3.9))
print(int(True))
print(float("3.2"))
print(float(3))
print(str(3))
print(str(3.25))
print(str(True))

# Truthiness. Zero and the empty string are falsy; a non-empty string is
# truthy. Rust's `!= 0` only works for integers.
print(bool(0))
print(bool(1))
print(bool(0.0))
print(bool(0.5))
print(bool(""))
print(bool("x"))

# json.dumps on a value computed at run time. The compile-time literal path
# renders during lowering; anything the program builds needs a real serializer.
import json

nums = [1, 2, 3]
print(json.dumps(nums))
jname = "Tarvos"
print(json.dumps(jname))
jnum = 7
print(json.dumps(jnum))
jfloat = 1.5
print(json.dumps(jfloat))
jbool = True
print(json.dumps(jbool))
counts = {"a": 1, "b": 2}
print(json.dumps(counts))
