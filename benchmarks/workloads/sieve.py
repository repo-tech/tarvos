def count_primes(n: int) -> int:
    primes_found = 0
    for x in range(2, n):
        is_prime = 1
        for d in range(2, x):
            if d * d > x:
                d = x + 1
            elif x % d == 0:
                is_prime = 0
                d = x + 1
        if is_prime == 1:
            primes_found = primes_found + 1
    return primes_found

print(count_primes(3000))
