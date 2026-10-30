use std::cmp::Ordering;

/// Compares two strings using natural human alphanumeric ordering
/// (e.g., "page2.jpg" < "page10.jpg" and "001.png" < "002.png").
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut chars_a = a.chars().peekable();
    let mut chars_b = b.chars().peekable();

    loop {
        match (chars_a.peek(), chars_b.peek()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(&ca), Some(&cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let mut num_a = String::new();
                    let mut num_b = String::new();

                    while let Some(&d) = chars_a.peek() {
                        if d.is_ascii_digit() {
                            num_a.push(d);
                            chars_a.next();
                        } else {
                            break;
                        }
                    }
                    while let Some(&d) = chars_b.peek() {
                        if d.is_ascii_digit() {
                            num_b.push(d);
                            chars_b.next();
                        } else {
                            break;
                        }
                    }

                    let trimmed_a = num_a.trim_start_matches('0');
                    let trimmed_b = num_b.trim_start_matches('0');

                    match trimmed_a.len().cmp(&trimmed_b.len()) {
                        Ordering::Equal => match trimmed_a.cmp(trimmed_b) {
                            Ordering::Equal => {
                                // Fewer leading zeros comes later if numerical values match
                                match num_a.len().cmp(&num_b.len()) {
                                    Ordering::Equal => continue,
                                    ord => return ord,
                                }
                            }
                            ord => return ord,
                        },
                        ord => return ord,
                    }
                } else {
                    let la = ca.to_ascii_lowercase();
                    let lb = cb.to_ascii_lowercase();
                    match la.cmp(&lb) {
                        Ordering::Equal => {
                            chars_a.next();
                            chars_b.next();
                        }
                        ord => return ord,
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_natural_sort_pages() {
        let mut pages = vec!["10.jpg", "2.jpg", "1.jpg", "002.jpg", "20.jpg"];
        pages.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(pages, vec!["1.jpg", "2.jpg", "002.jpg", "10.jpg", "20.jpg"]);
    }
}
