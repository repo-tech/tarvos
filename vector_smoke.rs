#![allow(unused_mut, unused_variables, dead_code, unused_parens, unused_assignments)]

#[inline(always)]
fn sum_to(n: i64) -> i64 {
    let mut total = 0_i64;
    // Tarvos explicit four-lane contiguous reduction; scalar tail preserves parity.
    if std::arch::is_x86_feature_detected!("avx2") {
        let mut __tarvos_base = 1_i64;
        while __tarvos_base + 4 <= n {
            total += __tarvos_base;
            total += __tarvos_base + 1;
            total += __tarvos_base + 2;
            total += __tarvos_base + 3;
            __tarvos_base += 4;
        }
        for i in __tarvos_base..n { total += i; }
    } else {
        for i in 1_i64..n { total += i; }
    }
    return total;
}

fn main() {
    println!("{}", sum_to(101_i64));
}
