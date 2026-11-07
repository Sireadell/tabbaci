#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Ans {
    T,
    F,
    U,
}

impl Ans {
    pub fn parse(s: &str) -> Ans {
        match s.trim() {
            "TRUE" => Ans::T,
            "FALSE" => Ans::F,
            _ => Ans::U,
        }
    }
    pub fn letter(self) -> &'static str {
        match self {
            Ans::T => "T",
            Ans::F => "F",
            Ans::U => "U",
        }
    }
}

/// Prose sentence: outcome letter from three judge answers. Unsigned or unparseable counts as U before this call.
/// Returns (outcome, weak) where weak means 2 of 3 with one UNSURE.
pub fn prose(a: &[Ans]) -> (&'static str, bool) {
    let t = a.iter().filter(|x| **x == Ans::T).count();
    let f = a.iter().filter(|x| **x == Ans::F).count();
    let u = a.len() - t - f;
    match (t, f, u) {
        (3, 0, 0) => ("T", false),
        (0, 3, 0) => ("F", false),
        (2, 0, 1) => ("T", true),
        (0, 2, 1) => ("F", true),
        (2, 1, 0) | (1, 2, 0) => ("D", false),
        _ => ("U", false),
    }
}

/// Number sentence: the numbers decide. Judges only confirm the sentence states the number.
/// `numbers_say_true` is whether the sources agree with the stated number within tolerance.
pub fn data(a: &[Ans], numbers_say_true: bool) -> (&'static str, bool) {
    let t = a.iter().filter(|x| **x == Ans::T).count();
    let f = a.iter().filter(|x| **x == Ans::F).count();
    let u = a.len() - t - f;
    let decisive = if numbers_say_true { "T" } else { "F" };
    match (t, f, u) {
        (3, 0, 0) => (decisive, false),
        (2, 0, 1) => (decisive, true),
        (2, 1, 0) | (1, 2, 0) => ("D", false),
        (0, 3, 0) | (0, 2, 1) => ("D", false),
        _ => ("U", false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Ans::*;
    #[test]
    fn prose_table() {
        assert_eq!(prose(&[T, T, T]), ("T", false));
        assert_eq!(prose(&[F, F, F]), ("F", false));
        assert_eq!(prose(&[T, T, U]), ("T", true));
        assert_eq!(prose(&[F, U, F]), ("F", true));
        assert_eq!(prose(&[T, T, F]).0, "D");
        assert_eq!(prose(&[F, T, F]).0, "D");
        assert_eq!(prose(&[T, U, U]).0, "U");
        assert_eq!(prose(&[T, F, U]).0, "U");
        assert_eq!(prose(&[U, U, U]).0, "U");
    }
    #[test]
    fn data_table() {
        assert_eq!(data(&[T, T, T], true), ("T", false));
        assert_eq!(data(&[T, T, T], false), ("F", false));
        assert_eq!(data(&[T, T, U], true), ("T", true));
        assert_eq!(data(&[T, T, F], true).0, "D");
        assert_eq!(data(&[F, F, F], true).0, "D");
        assert_eq!(data(&[U, U, T], true).0, "U");
    }
}
