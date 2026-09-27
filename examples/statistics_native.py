"""Showcase: the native statistics capability.

    tarvos build examples/statistics.py

compiles to a native executable with no Python present. Every value below is
compared against CPython by tests/corpus/40_statistics.py.
"""
import statistics

values = [10, 20, 30, 40, 50]
print(statistics.mean(values))
print(statistics.median(values))

grades = [88.0, 92.0, 79.0, 93.0, 85.0, 91.0]
print(statistics.mean(grades))
print(statistics.median(grades))
print(statistics.stdev(grades))
print(statistics.variance(grades))

# mode returns an element of the input, so this is the int 2, not 2.0
tallest = [1, 2, 2, 3, 3, 4]
print(statistics.mode(tallest))
print(statistics.multimode(tallest))

# geometric_mean reduces through logarithms, so this is exactly 4.0
squares = [1.0, 4.0, 16.0]
print(statistics.geometric_mean(squares))
print(statistics.harmonic_mean(squares))
