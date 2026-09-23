/// One-line rendering of a session task for the board: a title or a first
/// prompt arrives with embedded newlines and indentation, which would otherwise
/// break the row it is drawn on.
pub fn one_line(s: &str, max: usize) -> String {
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return collapsed;
    }
    if collapsed.chars().count() <= max {
        return collapsed;
    }
    let cut: String = collapsed.chars().take(max).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_whitespace_and_trims() {
        assert_eq!(one_line("  perbaiki\n\tlogin   page \n", 120), "perbaiki login page");
    }

    #[test]
    fn leaves_a_short_string_untouched() {
        assert_eq!(one_line("tambah tombol", 120), "tambah tombol");
    }

    #[test]
    fn cuts_on_a_char_boundary_and_adds_an_ellipsis() {
        assert_eq!(one_line("abcdef", 3), "abc…");
        // Each of these is one char, so a byte-wise cut would panic.
        assert_eq!(one_line("日本語のタスク", 3), "日本語…");
    }

    #[test]
    fn exactly_max_is_not_cut() {
        assert_eq!(one_line("abcde", 5), "abcde");
    }

    #[test]
    fn blank_input_stays_blank_and_uncut() {
        assert_eq!(one_line("   \n ", 5), "");
    }
}
