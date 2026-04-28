#[test]
fn enabled() {
    assert_eq!(1, 1);
}

#[ignore]
#[test]
fn ignored_one() {
    assert_eq!(1, 2);
}

#[ignore = "flaky"]
#[test]
fn ignored_two() {
    assert_eq!(1, 2);
}
