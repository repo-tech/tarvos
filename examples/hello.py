# Tarvos AST Test Case: Prime Generator and Square Accumulator
def run_benchmark():
    limit = 50000
    primes = []
    
    # 1. Nested Loop & Math Operations (Prime Filter)
    for num in range(2, limit):
        is_prime = True
        # प्राइम चेक करने का लॉजिक (C++ या Rust इसे तुरंत ऑप्टिमाइज़ करते हैं)
        for i in range(2, int(num ** 0.5) + 1):
            if num % i == 0:
                is_prime = False
                break
        if is_prime:
            primes.append(num)
            
    # 2. List Transformation & Accumulation
    total_sum = 0
    for p in primes:
        total_sum += p * p
        
    print("Total Primes Found:", len(primes))
    print("Accumulated Square Sum:", total_sum)

# Execution
run_benchmark()
