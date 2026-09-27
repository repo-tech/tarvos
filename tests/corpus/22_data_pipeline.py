data = [5, 3, 8, 1, 9, 2, 7]
evens = []
odds = []
for v in data:
    if v % 2 == 0:
        evens.append(v)
    else:
        odds.append(v)
print(evens)
print(odds)
total = 0
for v in data:
    total = total + v
print(total)
best = data[0]
for v in data:
    if v > best:
        best = v
print(best)
counts = {}
for v in data:
    if v in counts:
        counts[v] = counts[v] + 1
    else:
        counts[v] = 1
print(counts[5])
print(counts[1])
fib = [0, 1]
for i in range(2, 20):
    fib.append(fib[i - 1] + fib[i - 2])
print(fib)
primes = []
for n in range(2, 30):
    is_prime = True
    d = 2
    while d * d <= n:
        if n % d == 0:
            is_prime = False
        d = d + 1
    if is_prime:
        primes.append(n)
print(primes)
s = 0
for v in data:
    s = s + v * 2
print(s)
