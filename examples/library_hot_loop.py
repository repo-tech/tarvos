import numpy as np
import pandas as pd

values = np.arange(10)
scaled = []
for index in range(len(values)):
    scaled.append(values[index] * 2)

for _, row in pd.DataFrame({"value": values}).iterrows():
    print(row)
