def a():
    try:
        x = 1
    except ValueError:
        pass


def b():
    try:
        x = 1
    except ValueError as e:
        print(e)
