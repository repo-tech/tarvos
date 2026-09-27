def grade(score):
    if score >= 90:
        return "A"
    elif score >= 80:
        return "B"
    elif score >= 70:
        return "C"
    elif score >= 60:
        return "D"
    else:
        return "F"


for s in [95, 85, 75, 65, 50]:
    print(grade(s))

x = 10
if x > 100:
    print("big")
elif x > 50:
    print("medium")
elif x > 5:
    print("small")
else:
    print("tiny")

n = 0
while n < 3:
    print(n)
    n = n + 1

for i in range(3):
    for j in range(3):
        if j == 1:
            continue
        if i == 2:
            break
        print(i * 10 + j)

count = 0
for i in range(10):
    if i % 3 == 0:
        continue
    count = count + 1
print(count)
