//! Port of .NET `System.Random` (the legacy seeded algorithm, Knuth subtractive).
//! Via auto-artifactarium `cs_rand.rs` <- TheLostTree/evergreen `key_bruteforce.rs`.
//! The game seeds this with the .NET-ticks-derived wall clock to make `client_rand_key`.

use super::constants::DOTNET_EPOCH_OFFSET_SECS;

const MBIG: i32 = i32::MAX;
const MSEED: i32 = 161_803_398;

pub struct CsRandom {
    inext: usize,
    inextp: usize,
    seed_array: [i32; 56],
}

impl CsRandom {
    pub fn new(seed: i32) -> Self {
        let subtraction = if seed == i32::MIN { MBIG } else { seed.abs() };
        let mut seed_array = [0i32; 56];
        let mut mj = MSEED - subtraction;
        seed_array[55] = mj;
        let mut mk = 1;
        for i in 1..55 {
            let ii = 21 * i % 55;
            seed_array[ii] = mk;
            mk = mj - mk;
            if mk < 0 {
                mk += MBIG;
            }
            mj = seed_array[ii];
        }
        for _ in 1..5 {
            for i in 1..56 {
                seed_array[i] = seed_array[i].wrapping_sub(seed_array[1 + (i + 30) % 55]);
                if seed_array[i] < 0 {
                    seed_array[i] += MBIG;
                }
            }
        }
        Self {
            inext: 0,
            inextp: 21,
            seed_array,
        }
    }

    fn internal_sample(&mut self) -> i32 {
        let mut loc_inext = self.inext + 1;
        let mut loc_inextp = self.inextp + 1;
        if loc_inext >= 56 {
            loc_inext = 1;
        }
        if loc_inextp >= 56 {
            loc_inextp = 1;
        }
        let mut ret = self.seed_array[loc_inext].wrapping_sub(self.seed_array[loc_inextp]);
        if ret == MBIG {
            ret -= 1;
        }
        if ret < 0 {
            ret += MBIG;
        }
        self.seed_array[loc_inext] = ret;
        self.inext = loc_inext;
        self.inextp = loc_inextp;
        ret
    }

    /// `Random.Next()`
    pub fn next_i32(&mut self) -> i32 {
        self.internal_sample()
    }

    /// `Random.NextDouble()`
    pub fn next_double(&mut self) -> f64 {
        f64::from(self.internal_sample()) * (1.0 / f64::from(MBIG))
    }

    /// `Random.Next(maxValue)`
    pub fn next_max(&mut self, max_value: i32) -> i32 {
        (self.next_double() * f64::from(max_value)) as i32
    }
}

/// The seed the client derives from its clock: low 32 bits of .NET-epoch seconds.
pub fn seed_from_unix_secs(unix_secs: i64) -> i32 {
    ((unix_secs + DOTNET_EPOCH_OFFSET_SECS) & 0xFFFF_FFFF) as u32 as i32
}

/// `client_rand_key` for a given seed: `Next(int.MaxValue) << 32 | seed`.
pub fn client_rand_key(seed: i32) -> u64 {
    let hi = CsRandom::new(seed).next_max(i32::MAX) as u32;
    (u64::from(hi) << 32) | u64::from(seed as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference values from .NET: `new Random(42).Next()` twice, `new Random(0).Next()`.
    #[test]
    fn matches_dotnet() {
        let mut r = CsRandom::new(42);
        assert_eq!(r.next_i32(), 1_434_747_710);
        assert_eq!(r.next_i32(), 302_596_119);
        assert_eq!(CsRandom::new(0).next_i32(), 1_559_595_546);
    }

    #[test]
    fn min_seed_does_not_overflow() {
        let _ = CsRandom::new(i32::MIN).next_i32();
    }

    #[test]
    fn seed_truncates_to_i32() {
        // 1_700_000_000 + 62_135_596_800 = 63_835_596_800 = 0xE_DCE5_E800 -> low 32 bits 0xDCE5E800 (negative as i32)
        assert_eq!(seed_from_unix_secs(1_700_000_000), 0xDCE5_E800u32 as i32);
        assert!(seed_from_unix_secs(1_700_000_000) < 0);
    }

    #[test]
    fn key_low_word_is_seed() {
        let k = client_rand_key(-5);
        assert_eq!(k as u32, (-5i32) as u32);
    }
}
