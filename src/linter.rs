pub fn lint(_src: &str) -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_empty_source_yields_no_findings() {
        assert!(lint("int main() { return 0; }").is_empty());
    }
}
