pub fn escape_single_quote(text: &str) -> String {
    text.replace("'", "'\\''")
}
