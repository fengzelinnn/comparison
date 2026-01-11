use crate::group::RsaGroup;
use num_bigint::BigUint;
use num_integer::Integer;
use num_traits::{One, Zero};
use sha2::{Digest, Sha256};

pub fn hash_to_group(group: &RsaGroup, input: &[u8]) -> BigUint {
    let mut counter = 0u64;
    loop {
        let mut hasher = Sha256::new();
        hasher.update(b"residue");
        hasher.update(input);
        hasher.update(counter.to_be_bytes());
        let digest = hasher.finalize();
        let mut candidate = BigUint::from_bytes_be(&digest);
        candidate %= group.modulus();
        candidate = group.canonical(candidate);
        if candidate.is_zero() || candidate.is_one() {
            counter += 1;
            continue;
        }
        if candidate.gcd(group.modulus()).is_one() {
            return candidate;
        }
        counter += 1;
    }
}

pub fn hash_to_prime(g: &BigUint, y: &BigUint, bits: usize) -> BigUint {
    let mut counter = 0u64;
    loop {
        let mut seed_hasher = Sha256::new();
        seed_hasher.update(b"prime");
        seed_hasher.update(g.to_bytes_be());
        seed_hasher.update(y.to_bytes_be());
        seed_hasher.update(counter.to_be_bytes());
        let seed = seed_hasher.finalize();
        let mut candidate = expand_to_bits(&seed, bits);
        candidate.set_bit((bits - 1) as u64, true);
        candidate.set_bit(0, true);
        if is_probable_prime(&candidate, 16, &seed) {
            return candidate;
        }
        counter += 1;
    }
}

fn expand_to_bits(seed: &[u8], bits: usize) -> BigUint {
    let mut output = Vec::with_capacity((bits + 7) / 8);
    let mut counter = 0u64;
    while output.len() * 8 < bits {
        let mut hasher = Sha256::new();
        hasher.update(seed);
        hasher.update(counter.to_be_bytes());
        output.extend_from_slice(&hasher.finalize());
        counter += 1;
    }
    let mut candidate = BigUint::from_bytes_be(&output);
    let extra_bits = output.len() * 8 - bits;
    if extra_bits > 0 {
        candidate >>= extra_bits;
    }
    candidate
}

pub fn is_probable_prime(candidate: &BigUint, rounds: u32, seed: &[u8]) -> bool {
    if *candidate < BigUint::from(4u32) {
        return *candidate == BigUint::from(2u32) || *candidate == BigUint::from(3u32);
    }
    if candidate.is_even() {
        return false;
    }
    let (mut d, mut r) = (candidate - 1u32, 0u32);
    while d.is_even() {
        d >>= 1;
        r += 1;
    }
    let mut rng_state = Sha256::new();
    rng_state.update(seed);
    for i in 0..rounds {
        let mut hasher = Sha256::new();
        hasher.update(rng_state.clone().finalize());
        hasher.update(i.to_be_bytes());
        let digest = hasher.finalize();
        let a = BigUint::from_bytes_be(&digest) % (candidate - 3u32) + 2u32;
        if !miller_rabin_witness(candidate, &d, r, &a) {
            return false;
        }
    }
    true
}

fn miller_rabin_witness(n: &BigUint, d: &BigUint, r: u32, a: &BigUint) -> bool {
    let mut x = a.modpow(d, n);
    if x.is_one() || x == n - 1u32 {
        return true;
    }
    for _ in 1..r {
        x = x.modpow(&BigUint::from(2u32), n);
        if x == n - 1u32 {
            return true;
        }
    }
    false
}
