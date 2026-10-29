//! Human durations such as `500ms`, `30s`, `2m` or `1h`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A duration written the way people write it in test files. Stored in
/// milliseconds; serialized back to the largest unit that divides it evenly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration {
    millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "invalid duration `{input}`: expected a whole number followed by ms, s, m or h, like `30s`"
)]
pub struct DurationError {
    input: String,
}

impl Duration {
    pub const fn from_millis(millis: u64) -> Self {
        Self { millis }
    }

    pub const fn from_secs(secs: u64) -> Self {
        Self {
            millis: secs * 1000,
        }
    }

    pub const fn as_millis(self) -> u64 {
        self.millis
    }

    pub const fn is_zero(self) -> bool {
        self.millis == 0
    }
}

impl From<Duration> for std::time::Duration {
    fn from(d: Duration) -> Self {
        std::time::Duration::from_millis(d.millis)
    }
}

impl FromStr for Duration {
    type Err = DurationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || DurationError {
            input: s.to_owned(),
        };
        let trimmed = s.trim();
        let split = trimmed
            .find(|c: char| !c.is_ascii_digit())
            .ok_or_else(err)?;
        let (number, unit) = trimmed.split_at(split);
        let value: u64 = number.parse().map_err(|_| err())?;
        let factor = match unit.trim() {
            "ms" => 1,
            "s" => 1_000,
            "m" => 60_000,
            "h" => 3_600_000,
            _ => return Err(err()),
        };
        let millis = value.checked_mul(factor).ok_or_else(err)?;
        Ok(Self { millis })
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ms = self.millis;
        match ms {
            0 => write!(f, "0s"),
            _ if ms.is_multiple_of(3_600_000) => write!(f, "{}h", ms / 3_600_000),
            _ if ms.is_multiple_of(60_000) => write!(f, "{}m", ms / 60_000),
            _ if ms.is_multiple_of(1_000) => write!(f, "{}s", ms / 1_000),
            _ => write!(f, "{ms}ms"),
        }
    }
}

impl Serialize for Duration {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Duration {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_unit() {
        assert_eq!("250ms".parse(), Ok(Duration::from_millis(250)));
        assert_eq!("30s".parse(), Ok(Duration::from_secs(30)));
        assert_eq!("2m".parse(), Ok(Duration::from_secs(120)));
        assert_eq!("1h".parse(), Ok(Duration::from_secs(3600)));
        assert_eq!(" 5 s ".parse(), Ok(Duration::from_secs(5)));
    }

    #[test]
    fn rejects_bad_input() {
        for bad in ["", "s", "10", "1.5s", "10d", "-1s", "99999999999999999999h"] {
            assert!(bad.parse::<Duration>().is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn displays_the_largest_even_unit() {
        assert_eq!(Duration::from_millis(1500).to_string(), "1500ms");
        assert_eq!(Duration::from_secs(90).to_string(), "90s");
        assert_eq!(Duration::from_secs(120).to_string(), "2m");
        assert_eq!(Duration::from_secs(7200).to_string(), "2h");
        assert_eq!(Duration::from_millis(0).to_string(), "0s");
    }
}
