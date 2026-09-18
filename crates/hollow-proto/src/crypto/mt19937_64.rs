//! MT19937-64, bit-exact with C++ `std::mt19937_64` (Matsumoto & Nishimura 2004).

const NN: usize = 312;
const MM: usize = 156;
const MATRIX_A: u64 = 0xB502_6F5A_A966_19E9;
const UM: u64 = 0xFFFF_FFFF_8000_0000;
const LM: u64 = 0x7FFF_FFFF;

pub struct Mt19937_64 {
    mt: [u64; NN],
    mti: usize,
}

impl Mt19937_64 {
    pub fn new(seed: u64) -> Self {
        let mut mt = [0u64; NN];
        mt[0] = seed;
        for i in 1..NN {
            mt[i] = 6_364_136_223_846_793_005u64
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 62))
                .wrapping_add(i as u64);
        }
        Self { mt, mti: NN }
    }

    fn twist(&mut self) {
        let mag01 = |x: u64| if x & 1 == 0 { 0 } else { MATRIX_A };
        for i in 0..NN - MM {
            let x = (self.mt[i] & UM) | (self.mt[i + 1] & LM);
            self.mt[i] = self.mt[i + MM] ^ (x >> 1) ^ mag01(x);
        }
        for i in NN - MM..NN - 1 {
            let x = (self.mt[i] & UM) | (self.mt[i + 1] & LM);
            self.mt[i] = self.mt[i + MM - NN] ^ (x >> 1) ^ mag01(x);
        }
        let x = (self.mt[NN - 1] & UM) | (self.mt[0] & LM);
        self.mt[NN - 1] = self.mt[MM - 1] ^ (x >> 1) ^ mag01(x);
        self.mti = 0;
    }

    pub fn next_u64(&mut self) -> u64 {
        if self.mti >= NN {
            self.twist();
        }
        let mut x = self.mt[self.mti];
        self.mti += 1;
        x ^= (x >> 29) & 0x5555_5555_5555_5555;
        x ^= (x << 17) & 0x71D6_7FFF_EDA6_0000;
        x ^= (x << 37) & 0xFFF7_EEE0_0000_0000;
        x ^= x >> 43;
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C++ standard [rand.eng.mers]: the 10000th invocation of a
    /// default-constructed (seed 5489) `std::mt19937_64` yields this value.
    #[test]
    fn matches_std_mt19937_64() {
        let mut rng = Mt19937_64::new(5489);
        let mut last = 0;
        for _ in 0..10_000 {
            last = rng.next_u64();
        }
        assert_eq!(last, 9_981_545_732_273_789_042);
    }
}
