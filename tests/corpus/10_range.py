xs = []
for i in range(5):
    xs.append(i)
print(xs)
for i in range(2, 9):
    xs.append(i)
print(xs)
for i in range(10, 0, -2):
    xs.append(i)
print(xs)
for i in range(0, 10, 3):
    xs.append(i)
print(xs)
for i in range(0, 10, 7):
    xs.append(i)
print(xs)
for i in range(5, 5):
    xs.append(i)
print(xs)
for i in range(9, 0, -3):
    xs.append(i)
print(xs)
for i in range(0, 1000000, 250000):
    xs.append(i)
print(xs)
n = len(xs)
print(n)
total = 0
for i in range(1, 101):
    total = total + i
print(total)
