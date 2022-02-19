#[contract]
module 0x2::M {
    struct S has drop {
      a: u64,
      b: bool,
      c: S2
    }

    struct S2 has drop {
        x: u128
    }

    #[callable]
	fun pack_S2(x: u128): S2 {
	    S2{x}
	}

    #[callable]
    fun pack_S(a: u64, b: bool): S {
        S{a, b, c: pack_S2((a as u128))}
    }

    // #[callable]
    fun read_S(s: &S): u64 {
        s.a + (s.c.x as u64)
    }

    // #[callable]
    fun write_S(s: &mut S, v: u64) {
        s.a = v;
        s.c.x = (s.a as u128);
    }

    #[callable]
    fun read_and_write_s(): S {
        let s = pack_S(1, false);
        let x = read_S(&s);
        write_S(&mut s, x);
        s
    }

    #[callable]
    fun unpack(s: S): S2 {
        let S{a: _a, b: _b, c} = s;
        c
    }

    #[callable]
    fun destroy() {
        let _s = pack_S(1, false);
    }

}
