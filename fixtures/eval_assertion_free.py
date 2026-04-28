import pytest


# Should NOT fire — has assert.
def test_good():
    x = 2 + 2
    assert x == 4


# Should fire — empty body (just pass).
def test_empty():
    pass


# Should fire — code but no assert.
def test_forgot():
    x = 2 + 2
    print(x)


# Should NOT fire — uses pytest.raises.
def test_raises():
    with pytest.raises(ValueError):
        int("not a number")


# Should NOT fire — unittest-style assertX method.
class TestSomething:
    def test_method(self):
        x = 1
        self.assertEqual(x, 1)


# Should fire — class-method test with no assertion.
class TestForgot:
    def test_method(self):
        x = 1
        _ = x


# Not a test — helper function.
def helper():
    return 1
