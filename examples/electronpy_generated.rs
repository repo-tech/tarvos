fn main() {
    let mut total = 0_i64;
    for i in (0..10000000_i64) {
        total = (total + i);
    }
    println!("{:?}", total);
}
