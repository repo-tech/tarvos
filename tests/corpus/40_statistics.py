"""statistics module: CPython parity for the natively supported subset.

Every line here is compared byte-for-byte against CPython by
benchmarks/difftest.py, so this file only uses constructs the native
backend actually compiles (no try/except, which still falls back to the
Python compatibility launcher).
"""
import statistics

ints = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
floats = [10.0, 20.0, 30.0, 40.0, 50.0]
odd = [3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0]
repeated = [1.0, 2.0, 2.0, 3.0, 3.0, 4.0]
singletons = [7.0]
negative = [-4.0, -1.0, 0.0, 2.0, 5.0]
# geometric_mean needs strictly positive input. A zero here would make this
# case version-dependent: CPython 3.12 raises StatisticsError for a zero, while
# 3.13 accepts it and returns 0.0. Tarvos targets 3.12, but the differential
# harness also runs on whatever CPython the developer has, so a case that
# encodes one version's behaviour fails on the other. The rejection path is
# covered by a unit test instead, where the expected version is explicit.
positives = [1.0, 4.0, 16.0]
uneven = [1, 2, 3, 4, 5, 6, 7]
modes_int = [1, 2, 2, 3, 3, 4]

# mean / fmean
print(statistics.mean(ints))
print(statistics.mean(floats))
print(statistics.fmean(ints))
print(statistics.fmean(floats))
print(statistics.mean(singletons))
print(statistics.mean(negative))
# mean of all-integer input that divides evenly is an int in CPython, not a float
whole = [10, 20, 30, 40, 50]
print(statistics.mean(whole))
print(statistics.fmean(whole))
single_int = [7]
print(statistics.mean(single_int))
print(statistics.fmean(single_int))

# geometric_mean / harmonic_mean
print(statistics.geometric_mean(positives))
print(statistics.geometric_mean(floats))
print(statistics.harmonic_mean(floats))

# median family. median_low/median_high return an element of the input, so an
# int list must yield an int, not 2.0.
print(statistics.median(ints))
print(statistics.median(uneven))
print(statistics.median(odd))
print(statistics.median_low(ints))
print(statistics.median_low(uneven))
print(statistics.median_high(ints))
print(statistics.median_high(uneven))
print(statistics.median_low(floats))
print(statistics.median_high(odd))

# mode / multimode, including first-appearance ordering
print(statistics.mode(repeated))
print(statistics.mode(odd))
print(statistics.mode(modes_int))
print(statistics.multimode(repeated))
print(statistics.multimode(odd))
print(statistics.multimode(modes_int))
print(statistics.multimode(ints))

# dispersion
print(statistics.pvariance(floats))
print(statistics.variance(floats))
print(statistics.pstdev(floats))
print(statistics.stdev(floats))
print(statistics.variance(ints))
print(statistics.stdev(uneven))

# the call must borrow, not consume, a caller-owned list
source = [4.0, 1.0, 3.0, 2.0]
print(statistics.mean(source))
print(source)
print(statistics.median(source))
print(source)
