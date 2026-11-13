fn main() {
    let mut total: i64 = 0;

    for i in 0..10_000_000 {
        total += i;
    }

    println!("{}", total);
}