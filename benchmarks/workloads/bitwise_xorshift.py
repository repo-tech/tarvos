# Bitwise / LCG kernel: exercises ^, &, |, <<, >>, % and `//`.
# The loop-carried state dependency keeps the optimizer honest.
state = 123456789
counter = 0
for i in range(64):
    for j in range(64):
        for k in range(64):
            state = (state ^ (i + j + k + 1)) * 1103515245 + 12345
            state = state & 0xFFFFFFFF
            if state % 2 == 0:
                counter += 1
print(counter)
print(state | 1)
print(state >> 8)
print((state << 4) & 0xFFFF)
print(state // 1000000)