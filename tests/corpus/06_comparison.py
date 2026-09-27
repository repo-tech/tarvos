print(1 == 1)
print(1 != 2)
print(1 < 2)
print(2 <= 2)
print(3 > 4)
print(4 >= 4)
print(True and False)
print(True or False)
print(not True)
print(1 == 1.0)
print("a" == "a")
print("a" != "b")
x = 5
print(x > 3 and x < 10)
print(x == 5 or x == 6)
print(not (x == 5))
flag = True
print(flag and flag)
print(flag or flag)
# Python's `and`/`or` return an operand, not a bool, so `0 or 7` is 7. Tarvos
# reports that shape instead of emitting a non-bool `||`. Converting the
# operand with `bool()` first is the supported form.
y = 0
print(bool(y) or bool(7))
print(bool(y) and bool(7))
