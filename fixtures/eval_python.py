def small():
    return 42


def bare_except_demo():
    try:
        return 1 / 0
    except:
        return None


def typed_except_ok():
    try:
        return 1 / 0
    except ZeroDivisionError:
        return None


def has_print_calls():
    print("debug me")
    breakpoint()
    return 1
