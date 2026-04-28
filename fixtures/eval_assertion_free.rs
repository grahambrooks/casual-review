// Should NOT fire — has assert_eq.
#[test]
fn good_test() {
    let x = 2 + 2;
    assert_eq!(x, 4);
}

// Should fire — empty body.
#[test]
fn empty_test() {}

// Should fire — has code but no assertion.
#[test]
fn forgot_to_assert() {
    let x = 2 + 2;
    let _ = x;
}

// Should NOT fire — debug_assert counts.
#[test]
fn dbg_assert() {
    debug_assert_eq!(1, 1);
}

// Should NOT fire — tokio test variant.
#[tokio::test]
async fn async_good() {
    assert!(true);
}

// Should fire — async test with no assertion.
#[tokio::test]
async fn async_bad() {
    let _x = 42;
}

// Not a test — should NOT fire even though no assertion.
fn helper() {
    let _x = 1;
}
