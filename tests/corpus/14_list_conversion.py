xs = [1, 2, 3]
print(list(xs))
t = (4, 5, 6)
print(list(t))
print(list(range(4)))
print(list("abc"))
# An empty list literal carries no element type until something appends to it,
# so the type has to be established before `list()` can copy it.
e = []
e.append(9)
ys = list(e)
print(ys)
ys.append(10)
print(ys)
print(e)
nested = [[1, 2], [3, 4]]
print(nested)
print(nested[0])
print(nested[1][1])
flat = []
for row in nested:
    for v in row:
        flat.append(v)
print(flat)
