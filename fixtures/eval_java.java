package com.example;

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

// TODO: split this God class
public class Sample {

    public static final String AWS_KEY = "AKIAIOSFODNN7EXAMPLE";

    @Disabled("flaky")
    @Test
    public void disabledTest() {
        assert 1 == 2;
    }

    @Test
    public void enabledTest() {
        assert 1 == 1;
    }

    public void debugMethod(int x) {
        System.out.println("value=" + x);
        System.err.println("err=" + x);
        try {
            risky();
        } catch (Exception e) {
            // FIXME: actually handle this
        }
    }

    public void anotherCatch() {
        try {
            risky();
        } catch (Exception e) {
            e.printStackTrace();
        }
    }

    private void risky() throws Exception {
        throw new Exception("nope");
    }

    public void veryLong() {
        int a = 1;
        int b = 2;
        int c = 3;
        int d = 4;
        int e = 5;
        int f = 6;
        int g = 7;
        int h = 8;
        int i = 9;
        int j = 10;
        int k = 11;
        int l = 12;
        int m = 13;
        int n = 14;
        int o = 15;
        int p = 16;
        int q = 17;
        int r = 18;
        int s = 19;
        int t = 20;
        int u = 21;
        int v = 22;
        int w = 23;
        int x = 24;
        int y = 25;
        int z = 26;
        int aa = 27;
        int bb = 28;
        int cc = 29;
        int dd = 30;
        int ee = 31;
        int ff = 32;
        int gg = 33;
        int hh = 34;
        int ii = 35;
        int jj = 36;
        int kk = 37;
        int ll = 38;
        int mm = 39;
        int nn = 40;
        int oo = 41;
        int pp = 42;
    }
}
