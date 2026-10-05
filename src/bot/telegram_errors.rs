/// Telegram errors that mean we will never be able to DM this user until they
/// start (or unblock) the bot. Matching is case-insensitive.
pub fn is_unreachable_chat(err: &(impl ToString + ?Sized)) -> bool {
    let lower = err.to_string().to_ascii_lowercase();
    lower.contains("chat not found")
        || lower.contains("can't initiate conversation")
        || lower.contains("bot was blocked")
        || lower.contains("user is deactivated")
        || lower.contains("forbidden: bot")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_prod_proactive_failures() {
        assert!(is_unreachable_chat(
            r#"A Telegram's error: Unknown error: "Forbidden: bot can't initiate conversation with a user""#
        ));
        assert!(is_unreachable_chat(
            "A Telegram's error: Bad Request: chat not found"
        ));
        assert!(is_unreachable_chat(
            "Forbidden: bot was blocked by the user"
        ));
        assert!(!is_unreachable_chat("Bad Request: message is too long"));
        assert!(!is_unreachable_chat("network timeout"));
    }
}
