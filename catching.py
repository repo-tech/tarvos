print("=== Tarvos Exception and Control Flow Test ===")

# 1. Try-Except ZeroDivisionError and Finally
try:
    x = 10 / 0
    print("Should not reach here")
except ZeroDivisionError as e:
    print("Caught division by zero:", e)
finally:
    print("Finally block executed successfully")

# 2. Try-Except ValueError via raise
try:
    print("Raising ValueError...")
    raise ValueError("custom invalid value")
except ValueError as e:
    print("Caught ValueError:", e)

# 3. Loops with break and continue
total = 0
for i in range(10):
    if i == 2:
        continue
    if i == 7:
        break
    total = total + i
print("Loop total with continue & break:", total)

# 4. Slicing on lists
items = [10, 20, 30, 40, 50]
sub = items[1:4]
print("Slice length:", len(sub))
print("Slice elements:", sub[0], sub[1], sub[2])

print("=== All tests completed successfully ===")
