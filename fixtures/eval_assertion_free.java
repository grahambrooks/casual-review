package com.example;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

public class SampleTest {

    // Should NOT fire — has assertEquals.
    @Test
    public void goodTest() {
        int x = 2 + 2;
        assertEquals(4, x);
    }

    // Should fire — empty body.
    @Test
    public void emptyTest() {
    }

    // Should fire — body but no assertion.
    @Test
    public void forgotAssertion() {
        int x = 2 + 2;
        System.out.println(x);
    }

    // Should NOT fire — calls fail() which is also an assertion.
    @Test
    public void usesFail() {
        try {
            risky();
            fail("expected exception");
        } catch (Exception e) {
            // ok
        }
    }

    // Should NOT fire — Mockito verify counts.
    @Test
    public void verifies() {
        Object mock = new Object();
        // pretend Mockito.verify(mock).doSomething();
        verify(mock);
    }

    // Should NOT fire — assertThat (AssertJ/Hamcrest).
    @Test
    public void assertsThat() {
        assertThat(1).isEqualTo(1);
    }

    // Not a test — should not fire.
    public void helper() {
        int x = 1;
    }

    private static void verify(Object o) {}
    private static <T> Asserter<T> assertThat(T t) { return new Asserter<>(); }
    private static void risky() throws Exception { throw new Exception(); }

    static class Asserter<T> {
        public void isEqualTo(Object o) {}
    }
}
