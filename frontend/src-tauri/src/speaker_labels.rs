//! Source labels are provenance, not human names. Keep this predicate shared by
//! offline diarization and Enhance so a composite source cannot become a person.
pub fn is_generated_part(part: &str) -> bool {
    let part = part.trim().to_ascii_lowercase();
    matches!(part.as_str(), "" | "you" | "guest" | "unknown" | "mic" | "microphone" | "system" | "system audio")
        || part.strip_prefix("speaker ").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}
pub fn is_generated_label(label: &str) -> bool { label.split('+').all(is_generated_part) }
pub fn manual_names(label: &str) -> Vec<String> {
    label.split(" + ").map(str::trim).filter(|p| !is_generated_label(p)).map(str::to_string).collect()
}
pub fn is_manual_label(label: &str) -> bool { !manual_names(label).is_empty() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neutral_sources_and_composites_are_refinable_not_people() {
        for label in ["Microphone", "mic", "System audio", "Unknown", "Speaker 12", "Microphone + Guest", "You + Speaker 2", "System + Microphone", "Microphone+Guest", "Guest +You"] {
            assert!(!is_manual_label(label), "{label}");
            assert!(manual_names(label).is_empty());
        }
    }
    #[test]
    fn actual_manual_names_survive_source_composites() {
        assert_eq!(manual_names("Microphone + Alice + Speaker 2"), vec!["Alice"]);
        assert!(is_manual_label("Алексей"));
        assert!(is_manual_label("Speaker coordinator"));
        assert!(!is_generated_part("Speaker "));
    }
}
