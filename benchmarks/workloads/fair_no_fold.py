def compute(limit: int, bias: int) -> int:
    total = 0
    for i in range(limit + 1):
        total += i + bias
    return total


print(compute(10_000_000, 0))
