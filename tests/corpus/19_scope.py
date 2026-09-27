def outer_a():
    x = 1
    return x


def outer_b():
    x = 2
    return x


def shadow(x):
    y = x + 1
    x = 99
    return y + x


def early(a):
    if a > 0:
        return 1
    return 0


def many(a, b):
    if a > b:
        return "a"
    elif a == b:
        return "eq"
    else:
        return "b"


def loop_sum(n):
    total = 0
    for i in range(n):
        if i % 2 == 0:
            continue
        if i > 20:
            break
        total = total + i
    return total


def nested(x):
    total = 0
    for i in range(x):
        for j in range(x):
            if j > i:
                total = total + 1
    return total


print(outer_a())
print(outer_b())
print(shadow(5))
print(early(1))
print(early(-1))
print(many(2, 1))
print(many(1, 1))
print(many(0, 5))
print(loop_sum(30))
print(nested(4))
