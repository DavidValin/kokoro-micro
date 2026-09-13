//! Digits -> English words, so number tokens can flow through the same
//! dictionary lookup as any other word instead of needing their own phoneme
//! table.
//!
//! Handles plain cardinals ("24" -> "twenty four"), decimals ("3.5" ->
//! "three point five"), ordinals ("3rd" -> "third"), and a `$`/`£`/`€` prefix
//! ("$5.50" -> "five dollars and fifty cents"). Anything past the low
//! trillions, or with more than one decimal point, is not a number this
//! module can read - unusual enough in spoken text that returning `None` and
//! letting the caller fall through to a letter-by-letter reading is an
//! acceptable trade for not maintaining an unbounded scale-word table.

const ONES: [&str; 20] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const SCALES: [(u64, &str); 4] = [
    (1_000_000_000_000, "trillion"),
    (1_000_000_000, "billion"),
    (1_000_000, "million"),
    (1_000, "thousand"),
];

fn under_thousand(n: u64, out: &mut Vec<&'static str>) {
    if n >= 100 {
        out.push(ONES[(n / 100) as usize]);
        out.push("hundred");
        let rest = n % 100;
        if rest > 0 {
            under_thousand(rest, out);
        }
        return;
    }
    if n < 20 {
        if n > 0 {
            out.push(ONES[n as usize]);
        }
        return;
    }
    out.push(TENS[(n / 10) as usize]);
    if n % 10 > 0 {
        out.push(ONES[(n % 10) as usize]);
    }
}

fn cardinal_words(n: u64) -> Vec<&'static str> {
    if n == 0 {
        return vec!["zero"];
    }
    let mut out = Vec::new();
    let mut remaining = n;
    for &(scale, name) in &SCALES {
        if remaining >= scale {
            let count = remaining / scale;
            under_thousand(count, &mut out);
            out.push(name);
            remaining %= scale;
        }
    }
    if remaining > 0 {
        under_thousand(remaining, &mut out);
    }
    out
}

/// The last word of a cardinal, turned into an ordinal, per the usual
/// English spelling irregularities (one -> first, five -> fifth, twenty ->
/// twentieth, ...).
fn ordinalize(last: &str) -> String {
    match last {
        "one" => "first".into(),
        "two" => "second".into(),
        "three" => "third".into(),
        "five" => "fifth".into(),
        "eight" => "eighth".into(),
        "nine" => "ninth".into(),
        "twelve" => "twelfth".into(),
        _ if last.ends_with('y') => format!("{}ieth", &last[..last.len() - 1]),
        _ => format!("{last}th"),
    }
}

fn parse_int(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Expand a numeric token (as matched by `super::looks_numeric`) into a
/// space-separated string of English words.
pub(crate) fn expand(token: &str) -> Option<String> {
    let negative = token.starts_with('-');
    let token = token.trim_start_matches('-');

    let currency = match token.chars().next() {
        Some('$') => Some(("dollar", "cent")),
        Some('£') => Some(("pound", "pence")),
        Some('€') => Some(("euro", "cent")),
        _ => None,
    };
    let token = if currency.is_some() { &token[1..] } else { token };

    let ordinal_suffix = ["st", "nd", "rd", "th"]
        .iter()
        .find(|suf| token.ends_with(*suf) && token.len() > suf.len());
    let token = match ordinal_suffix {
        Some(suf) => &token[..token.len() - suf.len()],
        None => token,
    };

    let mut words: Vec<String> = Vec::new();
    if negative {
        words.push("minus".to_string());
    }

    let cleaned: String = token.chars().filter(|&c| c != ',').collect();
    let (int_part, frac_part) = match cleaned.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (cleaned.as_str(), None),
    };
    if frac_part.is_some_and(|f| f.contains('.')) {
        return None; // more than one decimal point: not a number we read
    }

    let int_value = parse_int(int_part)?;
    let mut int_words: Vec<&'static str> = cardinal_words(int_value);
    if let Some(last) = int_words.pop() {
        if ordinal_suffix.is_some() && frac_part.is_none() {
            words.extend(int_words.iter().map(|s| s.to_string()));
            words.push(ordinalize(last));
        } else {
            int_words.push(last);
            words.extend(int_words.iter().map(|s| s.to_string()));
        }
    }

    if let Some((name, _)) = currency {
        words.push(if int_value == 1 { name.into() } else { format!("{name}s") });
    }

    if let Some(frac) = frac_part {
        if !frac.is_empty() {
            if let Some((_, cent_name)) = currency {
                let cents = parse_int(frac)?;
                let cents = if frac.len() == 1 { cents * 10 } else { cents };
                words.push("and".into());
                words.extend(cardinal_words(cents).iter().map(|s| s.to_string()));
                words.push(if cents == 1 { cent_name.into() } else { format!("{cent_name}s") });
            } else {
                words.push("point".into());
                for c in frac.chars() {
                    let d = c.to_digit(10)?;
                    words.push(ONES[d as usize].to_string());
                }
            }
        }
    }

    Some(words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_cardinals() {
        assert_eq!(expand("24").as_deref(), Some("twenty four"));
        assert_eq!(expand("100").as_deref(), Some("one hundred"));
        assert_eq!(expand("1005").as_deref(), Some("one thousand five"));
    }

    #[test]
    fn large_cardinals_with_thousands_separators() {
        assert_eq!(expand("1,234").as_deref(), Some("one thousand two hundred thirty four"));
    }

    #[test]
    fn decimals() {
        assert_eq!(expand("3.5").as_deref(), Some("three point five"));
    }

    #[test]
    fn ordinals() {
        assert_eq!(expand("1st").as_deref(), Some("first"));
        assert_eq!(expand("22nd").as_deref(), Some("twenty second"));
        assert_eq!(expand("100th").as_deref(), Some("one hundredth"));
    }

    #[test]
    fn currency() {
        assert_eq!(expand("$5").as_deref(), Some("five dollars"));
        assert_eq!(expand("$5.50").as_deref(), Some("five dollars and fifty cents"));
        assert_eq!(expand("$1").as_deref(), Some("one dollar"));
    }

    #[test]
    fn negative_numbers() {
        assert_eq!(expand("-7").as_deref(), Some("minus seven"));
    }
}
