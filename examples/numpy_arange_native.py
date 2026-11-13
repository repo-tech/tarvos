import numpy as np

values = np.arange(8)
total = 0
for index in range(len(values)):
    total += values[index]

print(total)
