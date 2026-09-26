def multiply(size):
    left = [[i + j for j in range(size)] for i in range(size)]
    right = [[i * j for j in range(size)] for i in range(size)]
    result = [[0 for _ in range(size)] for _ in range(size)]
    for i in range(size):
        for j in range(size):
            for k in range(size):
                result[i][j] += left[i][k] * right[k][j]
    checksum = 0
    for row in result:
        for value in row:
            checksum += value
    return checksum

print(multiply(12))