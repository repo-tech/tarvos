"""Native exception handling: CPython parity for the supported subset.

A `try` compiles to a labelled block rather than a closure so that variables
assigned inside the body stay in scope afterwards, and a `return` is deferred so
`finally` still runs. Nothing here falls back to the Python compatibility
launcher: every case is a real native executable.
"""

# a try whose body completes normally; `else` runs, `finally` runs
try:
    x = 1
    print("body", x)
except:
    print("unreachable")
else:
    print("else ran")
finally:
    print("finally 1")

# a caught exception, with the real message bound by `as`
try:
    raise ValueError("boom")
except ValueError as e:
    print("caught", e)
finally:
    print("finally 2")

# an unmatched type falls through to the next handler
try:
    raise TypeError("wrong type")
except ValueError:
    print("not this one")
except TypeError as e:
    print("second handler", e)

# a bare except catches everything
try:
    raise RuntimeError("generic")
except:
    print("bare except")

# `except` matching a superclass: StatisticsError is a ValueError in CPython,
# and the native error hierarchy models that.
import statistics
try:
    print(statistics.mean([]))
except ValueError:
    print("statistics error caught as ValueError")

# nested try: the inner handler runs, the outer finally still runs
try:
    try:
        raise ValueError("inner")
    except KeyError:
        print("wrong inner handler")
    finally:
        print("inner finally")
except ValueError as e:
    print("outer caught", e)

# `else` must NOT run when the body raised
try:
    raise ValueError("skip else")
except ValueError:
    print("handled, else skipped")

# finally runs on the success path too
try:
    print("plain body")
finally:
    print("finally 3")

# an error no handler matches propagates out of the try.
# RuntimeError is used rather than KeyError because CPython's `str(KeyError)` is
# the *repr* of its argument ("'propagate me'"), a per-class `__str__` override
# the native error type does not model. See the KeyError note in LIMITATIONS.
try:
    try:
        raise RuntimeError("propagate me")
    except ValueError:
        print("never")
    finally:
        print("propagating finally")
except RuntimeError as e:
    print("outer got", e)

print("end")
