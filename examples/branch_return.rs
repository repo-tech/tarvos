fn main() {
    fn max_of(x: i64, y: i64) -> i64 {
        if (x > y) {
            return x;
        } else {
            return y;
        }
    }
    println!("{:?}", max_of(3_i64, 7_i64));
    println!("{:?}", max_of(9_i64, 2_i64));
}
