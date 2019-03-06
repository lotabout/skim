use unicode_width::UnicodeWidthChar;

pub fn escape_single_quote(text: &str) -> String {
    text.replace("'", "'\\''")
}

/// return an array, arr[i] store the display width till char[i]
pub fn accumulate_text_width(text: &str, tabstop: usize) -> Vec<usize> {
    let mut ret = Vec::new();
    let mut w = 0;
    for ch in text.chars() {
        w += if ch == '\t' {
            tabstop - (w % tabstop)
        } else {
            ch.width().unwrap_or(2)
        };
        ret.push(w);
    }
    ret
}

/// "smartly" calculate the "start" position of the string in order to show the matched contents
/// for example, if the match appear in the end of a long string, we need to show the right part.
/// ```
/// xxxxxxxxxxxxxxxxxxxxxxxxxxMMxxxxxMxxxxx
///               shift ->|               |
/// ```
///
/// return (left_shift, full_print_width)
pub fn reshape_string(
    text: &str,
    container_width: usize,
    match_start: usize,
    match_end: usize,
    tabstop: usize,
) -> (usize, usize) {
    if text.is_empty() {
        return (0, 0);
    }

    let acc_width = accumulate_text_width(text, tabstop);
    let full_width = acc_width[acc_width.len() - 1];
    if full_width <= container_width {
        return (0, full_width);
    }

    // w1, w2, w3 = len_before_matched, len_matched, len_after_matched
    let w1 = if match_start == 0 {
        0
    } else {
        acc_width[match_start - 1]
    };
    let w2 = if match_end >= acc_width.len() {
        full_width - w1
    } else {
        acc_width[match_end] - w1
    };
    let w3 = acc_width[acc_width.len() - 1] - w1 - w2;

    if (w1 > w3 && w2 + w3 <= container_width) || (w3 <= 2) {
        // right-fixed
        //(right_fixed(&acc_width, container_width), full_width)
        (full_width - container_width, full_width)
    } else if w1 <= w3 && w1 + w2 <= container_width {
        // left-fixed
        (0, full_width)
    } else {
        // left-right
        (acc_width[match_end] - container_width + 2, full_width)
    }
}

#[cfg(test)]
mod tests {
    use super::{accumulate_text_width, reshape_string};

    #[test]
    fn test_accumulate_text_width() {
        assert_eq!(accumulate_text_width("abcdefg", 8), vec![1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(accumulate_text_width("ab中de国g", 8), vec![1, 2, 4, 5, 6, 8, 9]);
        assert_eq!(accumulate_text_width("ab\tdefg", 8), vec![1, 2, 8, 9, 10, 11, 12]);
        assert_eq!(accumulate_text_width("ab中\te国g", 8), vec![1, 2, 4, 8, 9, 11, 12]);
    }

    #[test]
    fn test_reshape_string() {
        // no match, left fixed to 0
        assert_eq!(reshape_string("abc", 10, 0, 0, 8), (0, 3));
        assert_eq!(reshape_string("a\tbc", 8, 0, 0, 8), (0, 10));
        assert_eq!(reshape_string("a\tb\tc", 10, 0, 0, 8), (0, 17));
        assert_eq!(reshape_string("a\t中b\tc", 8, 0, 0, 8), (0, 17));
        assert_eq!(reshape_string("a\t中b\tc012345", 8, 0, 0, 8), (0, 23));
    }
}
