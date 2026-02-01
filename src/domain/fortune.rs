use sha2::{Sha256, Digest};
use chrono::{Utc, FixedOffset};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fortune {
    Daikichi,
    Kichi,
    Chukichi,
    Shokichi,
    Suekichi,
    Kyo,
    Daikyo,
}

impl Fortune {
    pub fn as_str(&self) -> &'static str {
        match self {
            Fortune::Daikichi => "daikichi",
            Fortune::Kichi => "kichi",
            Fortune::Chukichi => "chukichi",
            Fortune::Shokichi => "shokichi",
            Fortune::Suekichi => "suekichi",
            Fortune::Kyo => "kyo",
            Fortune::Daikyo => "daikyo",
        }
    }
}

impl fmt::Display for Fortune {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Fortune {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "daikichi" => Ok(Fortune::Daikichi),
            "kichi" => Ok(Fortune::Kichi),
            "chukichi" => Ok(Fortune::Chukichi),
            "shokichi" => Ok(Fortune::Shokichi),
            "suekichi" => Ok(Fortune::Suekichi),
            "kyo" => Ok(Fortune::Kyo),
            "daikyo" => Ok(Fortune::Daikyo),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FortuneDef {
    pub val: Fortune,
    pub label: &'static str,
    pub threshold: u32,
}

pub const FORTUNES: &[FortuneDef] = &[
    FortuneDef { val: Fortune::Daikichi, label: "大吉", threshold: 6 },   // 6%
    FortuneDef { val: Fortune::Kichi, label: "吉", threshold: 28 },     // 22%
    FortuneDef { val: Fortune::Chukichi, label: "中吉", threshold: 50 },  // 22%
    FortuneDef { val: Fortune::Shokichi, label: "小吉", threshold: 70 },  // 20%
    FortuneDef { val: Fortune::Suekichi, label: "末吉", threshold: 88 },  // 18%
    FortuneDef { val: Fortune::Kyo, label: "凶", threshold: 97 },       // 9%
    FortuneDef { val: Fortune::Daikyo, label: "大凶", threshold: 100 },   // 3%
];

pub fn get_daily_fortune(did: &str) -> Fortune {
    let jst_offset = FixedOffset::east_opt(9 * 3600).unwrap();
    let now_jst = Utc::now().with_timezone(&jst_offset);
    let date_str = now_jst.format("%Y-%m-%d").to_string();

    calculate_fortune(did, &date_str)
}

pub fn calculate_fortune(did: &str, date_str: &str) -> Fortune {
    let seed = format!("{}{}", did, date_str);

    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    let hash = hasher.finalize();

    // Read first 4 bytes as u32be
    let hash_val = u32::from_be_bytes(hash[0..4].try_into().unwrap());
    let val = hash_val % 100;

    for fortune in FORTUNES {
        if val < fortune.threshold {
            return fortune.val;
        }
    }
    Fortune::Kichi
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fortune_consistency() {
        let date = "2026-01-28";

        assert_eq!(calculate_fortune("did:plc:ragtjsm2j2vknwkz3zp4oxrd", date), Fortune::Daikichi);
        assert_eq!(calculate_fortune("did:plc:e7w52g22jjgr5g7y6j6y6", date), Fortune::Daikichi);
        assert_eq!(calculate_fortune("did:plc:test1234", date), Fortune::Chukichi);
    }
}
