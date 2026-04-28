import pytest


def test_enabled():
    assert 1 == 1


@pytest.mark.skip(reason="broken")
def test_one():
    assert 1 == 2


@pytest.mark.skipif(True, reason="env")
def test_two():
    assert 1 == 2
