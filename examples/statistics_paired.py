import statistics as s
d4 = [1.0, 2.0, 3.0, 4.0]
x = [1.0, 2.0, 3.0, 4.0]
y = [2.0, 4.0, 5.0, 4.0]
print(s.median_grouped(d4))
print(s.median_grouped(d4, 2.0))
print(s.quantiles(d4))
print(s.covariance(x, y))
print(s.correlation(x, y))
r = s.linear_regression(x, y)
print(r[0])
print(r[1])
