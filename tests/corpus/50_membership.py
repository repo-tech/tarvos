"""`in` / `not in` across every container Tarvos supports.

Membership was the one comparison operator the bridge dropped, so `7 in values`
fell back to the compatibility runtime. These cases pin the operand order
(the value is the needle, the container the haystack), the negation, and the
per-container spelling: a sequence and a set use `contains`, a map uses
`contains_key`, and a string uses `contains` for a substring.
"""

values = [5, 2, 9, 1, 7, 3]
print("in_list:", 7 in values)
print("not_present:", 42 in values)
print("not_in:", 42 not in values)
print("edge:", 9 in [1, 2, 3])

text = "Tarvos Python Compiler"
print("in_str:", "Python" in text)
print("str_absent:", "Zig" in text)
print("str_not_in:", "Zig" not in text)

letters = {1, 2, 3, 4}
print("in_set:", 3 in letters)
print("set_absent:", 9 in letters)