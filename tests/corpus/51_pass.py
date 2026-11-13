for i in range(5):
    if i % 2 == 0:
        pass
    else:
        print("odd:", i)

try:
    x = 10 / 2
    print("divided:", x)
except ZeroDivisionError:
    print("never")
finally:
    pass

print("nested:", 1 / 0 if False else "done")
print("pass works")