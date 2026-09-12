fn compute(limit: i64, bias: i64) -> i64 {
    let mut total = 0_i64;
    for i in 0..=limit {
        total += i + bias;
    }
    total
}

fn main() {
    println!("{}", compute(10_000_000, 0));
}
