def safe_floor(a: int, b: int) -> int:
    try:
        return a // b
    except ZeroDivisionError:
        return -1


def safe_true(a: float, b: float) -> float:
    try:
        return a / b
    except ZeroDivisionError:
        return -1.0


print(safe_floor(10, 2))
print(safe_floor(1, 0))
print(safe_floor(-7, 2))
print(safe_true(10.0, 2.0))
print(safe_true(1.0, 0.0))
