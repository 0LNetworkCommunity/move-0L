#[contract]
module 0x2::M {
    use Eth::Evm::sign;

    struct S has key {
      a: u64,
      c: S2
    }

    struct S2 has store {
        x: bool
    }

    fun publish(sg: &signer, a: u64) {
        let s = S{a, c: S2{x: true}};
        move_to<S>(sg, s)
    }
    #[evm_test]
    #[callable]
    fun test_publish() acquires S {
       publish(&sign(@3), 22);
       assert!(borrow_global<S>(@3).a == 22, 100);
    }

    #[callable]
    fun unpublish(a: address): S acquires S {
        move_from<S>(a)
    }

    #[callable]
    fun test(addr: address): bool {
        exists<S>(addr)
    }

    #[callable]
    fun increment_a(addr: address) acquires S{
        let r = borrow_global_mut<S>(addr);
        r.a = r.a + 1
    }
}
