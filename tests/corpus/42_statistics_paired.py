import statistics as s
d4 = [1.0, 2.0, 3.0, 4.0]
x = [1.0, 2.0, 3.0, 4.0]
y = [2.0, 4.0, 5.0, 4.0]
print(s.median_grouped(d4))
print(s.median_grouped(d4, 2.0))
print(s.median_grouped([1.0, 2.0, 3.0, 4.0, 5.0]))
print(s.median_grouped([1.0, 2.0]))
print(s.median_grouped([1.0]))
print(s.median_grouped([1.0, 2.0, 2.0, 3.0]))
print(s.median_grouped([5.0]))
print(s.quantiles(d4))
print(s.quantiles([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]))
print(s.quantiles([7.0]))
print(s.covariance(x, y))
print(s.correlation(x, y))
r = s.linear_regression(x, y)
print(r[0])
print(r[1])
print(s.covariance([-1.0, 0.0, 1.0], [1.0, 2.0, 3.0]))
print(s.correlation([1.0, 2.0, 3.0], [3.0, 2.0, 1.0]))
try:
    s.covariance([1.0, 2.0, 3.0], [1.0, 2.0])
except s.StatisticsError as e:
    print("mismatch caught")
try:
    s.covariance([1.0], [2.0])
except s.StatisticsError as e:
    print("too few caught")
try:
    s.correlation([1.0, 1.0], [1.0, 2.0])
except s.StatisticsError as e:
    print("constant caught")
try:
    s.linear_regression([1.0, 1.0, 1.0], [1.0, 2.0, 3.0])
except s.StatisticsError as e:
    print("degenerate caught")
try:
    s.median_grouped([])
except s.StatisticsError as e:
    print("empty grouped caught")
try:
    s.quantiles([])
except s.StatisticsError as e:
    print("empty quantiles caught")
