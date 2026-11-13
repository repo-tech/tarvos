fn main() {
    let x = 10;
    let y = 20;
    let z = 0;
    if (x < y) {
        let z = (x + y);
    } else {
        let z = (y - x);
    }
    println!("{:?}", z);
}
