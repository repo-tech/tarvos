#![allow(unused_mut, unused_variables, dead_code, unused_parens, unused_assignments)]

use std::collections::HashMap;

#[inline(always)]
fn run_benchmark() {
    let mut limit = 50000_i64;
    let mut primes = vec![];
    for num in (2_i64..50000_i64) {
        let mut is_prime = true;
        for i in (2_i64..((num as f64).powf(0.5_f64 as f64) as i64 + 1_i64)) {
            if ((num % i) == 0_i64) {
                is_prime = false;
                break;
            }
        }
        if is_prime {
            primes.push(num);
        }
    }
    let mut total_sum = 0_i64;
    for p in primes.iter().cloned() {
        total_sum = (total_sum + (p * p));
    }
    println!("{} {}", "Total Primes Found:".to_string(), primes.len());
    println!("{} {}", "Accumulated Square Sum:".to_string(), total_sum);
}

fn main() {
    run_benchmark();
}
