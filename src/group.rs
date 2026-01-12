use num_bigint::BigUint;
use num_traits::One;

#[derive(Debug, Clone)]
pub struct RsaGroup {
    n: BigUint,
}

impl RsaGroup {
    pub fn new(n: BigUint) -> Self {
        Self { n }
    }

    pub fn modulus(&self) -> &BigUint {
        &self.n
    }

    pub fn canonical(&self, value: BigUint) -> BigUint {
        let mut reduced = value % &self.n;
        let neg = &self.n - &reduced;
        if reduced > neg {
            reduced = neg;
        }
        reduced
    }

    pub fn mul(&self, a: &BigUint, b: &BigUint) -> BigUint {
        self.canonical((a * b) % &self.n)
    }

    pub fn sq(&self, a: &BigUint) -> BigUint {
        self.canonical((a * a) % &self.n)
    }

    pub fn one(&self) -> BigUint {
        BigUint::one()
    }
}
