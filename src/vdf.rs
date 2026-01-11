use crate::group::RsaGroup;
use crate::hash::{hash_to_group, hash_to_prime};
use num_bigint::BigUint;
use num_traits::{One, Zero};

pub struct VdfOutput {
    pub g: BigUint,
    pub y: BigUint,
    pub proof: BigUint,
}

pub fn eval(group: &RsaGroup, input: &[u8], t: u64, k: usize) -> VdfOutput {
    let g = hash_to_group(group, input);
    let y = repeated_square(group, &g, t);
    let prime = hash_to_prime(&g, &y, 2 * k);
    let proof = prove_alg4(group, &g, &prime, t);
    VdfOutput { g, y, proof }
}

pub fn verify(
    group: &RsaGroup,
    g: &BigUint,
    y: &BigUint,
    proof: &BigUint,
    t: u64,
    k: usize,
) -> bool {
    let prime = hash_to_prime(g, y, 2 * k);
    let r = pow_mod_two(t, &prime);
    let left = pow_group(group, proof, &prime);
    let right = group.mul(&left, &pow_group(group, g, &r));
    &right == y
}

fn repeated_square(group: &RsaGroup, base: &BigUint, t: u64) -> BigUint {
    let mut acc = base.clone();
    for _ in 0..t {
        acc = group.sq(&acc);
    }
    acc
}

pub fn prove_alg4(group: &RsaGroup, g: &BigUint, prime: &BigUint, t: u64) -> BigUint {
    let mut x = group.one();
    let mut r = BigUint::one();
    for _ in 0..t {
        let two_r = &r << 1;
        let b = if &two_r >= prime { 1u32 } else { 0u32 };
        r = &two_r % prime;
        x = group.sq(&x);
        if b == 1 {
            x = group.mul(&x, g);
        }
    }
    x
}

fn pow_mod_two(exp: u64, modulus: &BigUint) -> BigUint {
    let base = BigUint::from(2u32);
    base.modpow(&BigUint::from(exp), modulus)
}

fn pow_group(group: &RsaGroup, base: &BigUint, exp: &BigUint) -> BigUint {
    if exp.is_zero() {
        return group.one();
    }
    let mut result = group.one();
    let mut base_acc = base.clone();
    let mut e = exp.clone();
    while !e.is_zero() {
        if &e & BigUint::one() == BigUint::one() {
            result = group.mul(&result, &base_acc);
        }
        base_acc = group.sq(&base_acc);
        e >>= 1;
    }
    result
}
