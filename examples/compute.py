
def compute_total(limit: int) -> int:
    total = 0
    for i in range(limit):
        total += i * i
    return total

print(compute_total(1_000_000))