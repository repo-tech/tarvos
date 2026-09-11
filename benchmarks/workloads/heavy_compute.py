def compute_loop(iterations: int) -> int:
    result = 0
    for i in range(iterations):
        if i % 2 == 0:
            result = result + (i * 3) % 10007
        else:
            result = result - (i * 7) % 10007
    return result

print(compute_loop(5000000))
