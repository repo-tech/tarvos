total = 0
for i in range(1000):
    if i % 3 == 0:
        total = total + i
    else:
        total = total - 1
print(total)
