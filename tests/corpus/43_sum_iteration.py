# Regression: sum() over each iterable shape Tarvos emits.
#
# `sum(range(...))` used to compile to `(0..100).iter().sum()`, which is
# E0599: Rust's Range is an Iterator but has no `iter()` method. The three
# range shapes lower differently and only two are iterators:
#
#   range(a, b)            -> (a..b)            Iterator
#   range(a, b, <literal>) -> (a..b).step_by(n) Iterator
#   range(a, b, <dynamic>) -> tarvos_range(...) Vec<i64>
#
# so the third form still needs `.iter()`. Lists must keep `.iter()` too:
# `sum(xs)` does not consume xs in Python, and it is reprinted below to
# prove nothing was moved.

xs = [1, 2, 3]
print(sum(xs))
print(xs)

print(sum([]))
print(sum(range(100)))
print(sum(range(0, 100, 2)))
print(sum(range(5)))

# A step the optimizer cannot fold, to reach the Vec-returning helper.
step = int("3")
print(sum(range(0, 100, step)))

print(sum([0, 0, 0]))
print(sum([-5, 5, -1, 1]))
