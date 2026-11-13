from numba import njit


@njit(cache=True)
def compute() -> int:
    total = 0
    for i in range(10_000_001):
        total += i
    return total


if __name__ == "__main__":
    print(compute())
