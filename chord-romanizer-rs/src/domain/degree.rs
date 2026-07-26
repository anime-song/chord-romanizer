use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RomanDegree {
    I,
    Ii,
    Iii,
    Iv,
    V,
    Vi,
    Vii,
}

impl RomanDegree {
    pub const ALL: [Self; 7] = [
        Self::I,
        Self::Ii,
        Self::Iii,
        Self::Iv,
        Self::V,
        Self::Vi,
        Self::Vii,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::I => 0,
            Self::Ii => 1,
            Self::Iii => 2,
            Self::Iv => 3,
            Self::V => 4,
            Self::Vi => 5,
            Self::Vii => 6,
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "I" => Some(Self::I),
            "II" => Some(Self::Ii),
            "III" => Some(Self::Iii),
            "IV" => Some(Self::Iv),
            "V" => Some(Self::V),
            "VI" => Some(Self::Vi),
            "VII" => Some(Self::Vii),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::I => "I",
            Self::Ii => "II",
            Self::Iii => "III",
            Self::Iv => "IV",
            Self::V => "V",
            Self::Vi => "VI",
            Self::Vii => "VII",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Degree {
    pub accidental: i8,
    pub degree: RomanDegree,
}

impl Degree {
    pub const fn new(accidental: i8, degree: RomanDegree) -> Self {
        Self { accidental, degree }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut accidental = 0_i16;
        let mut body_start = 0;
        for (index, ch) in text.char_indices() {
            match ch {
                '#' => accidental += 1,
                'b' => accidental -= 1,
                _ => {
                    body_start = index;
                    break;
                }
            }
        }
        let accidental = i8::try_from(accidental).ok()?;
        let degree = RomanDegree::parse(&text[body_start..])?;
        Some(Self::new(accidental, degree))
    }
}

impl fmt::Display for Degree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (symbol, count) = if self.accidental >= 0 {
            ('#', self.accidental as usize)
        } else {
            ('b', self.accidental.unsigned_abs() as usize)
        };
        for _ in 0..count {
            write!(f, "{symbol}")?;
        }
        write!(f, "{}", self.degree.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degree_round_trip() {
        for value in ["I", "#IV", "bbIII", "VII"] {
            assert_eq!(Degree::parse(value).unwrap().to_string(), value);
        }
    }
}
