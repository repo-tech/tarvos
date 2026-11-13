def matrix_cpul(arg: int) -> int:
    checksum = 0
    for i in range(150):
        for j in range(150):
            for k in range(150):
                checksum = (checksum + (i * j + k)) % 1000007
    return checksum

print(matrix_cpul(1))
