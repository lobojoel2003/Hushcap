//! Clean-up of raw recogniser output.
//!
//! Whisper models occasionally "hallucinate" on silence or noise (classic
//! examples: "Thank you.", "Thanks for watching!", subtitle credits) and can
//! get stuck repeating a phrase. For people who rely on captions, a confident
//! wrong caption is worse than none, so these are filtered out here.

/// Normalise whitespace and strip recogniser artefacts. Returns `None` when
/// nothing meaningful remains.
pub fn clean(raw: &str) -> Option<String> {
    let mut text = raw.replace(['\n', '\r', '\t'], " ");
    for marker in [
        "[BLANK_AUDIO]",
        "[ Silence ]",
        "[SILENCE]",
        "(silence)",
        "[silence]",
        "[NO_SPEECH]",
        "<|endoftext|>",
    ] {
        text = text.replace(marker, " ");
    }
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let deduped = collapse_repeats(&collapsed);
    let trimmed = deduped.trim_matches(|c: char| c.is_whitespace() || c == '-' || c == '"');
    if trimmed.chars().any(|c| c.is_alphanumeric()) || is_sound_tag(trimmed) {
        Some(trimmed.to_string())
    } else {
        None
    }
}

/// `[Music]`, `(applause)` and similar non-speech descriptions. These are
/// kept: they tell a deaf or hard-of-hearing reader what is going on.
pub fn is_sound_tag(text: &str) -> bool {
    let t = text.trim();
    (t.starts_with('[') && t.ends_with(']'))
        || (t.starts_with('(') && t.ends_with(')'))
        || t.starts_with('♪')
}

/// Phrases Whisper is known to invent when given silence or noise.
const HALLUCINATIONS: &[&str] = &[
    "thank you",
    "thank you.",
    "thanks for watching",
    "thanks for watching!",
    "thank you for watching",
    "thank you for watching.",
    "thank you so much for watching",
    "please subscribe",
    "please like and subscribe",
    "subscribe to my channel",
    "subtitles by the amara.org community",
    "transcription by castingwords",
    "you",
    "bye.",
    "bye",
    "so",
    "okay.",
    "i'm sorry.",
    "...",
];

/// Whether a short result is probably invented rather than heard.
///
/// `no_speech_prob` is the recogniser's own estimate that the audio
/// contained no speech; `speech_ratio` is the fraction of the audio the
/// voice activity detector considered speech.
pub fn is_probable_hallucination(text: &str, no_speech_prob: f32, speech_ratio: f32) -> bool {
    let normalized = text.trim().to_lowercase();
    let known = HALLUCINATIONS.contains(&normalized.as_str())
        || normalized.contains("amara.org")
        || normalized.contains("subtitles by")
        || normalized.contains("captions by");
    if no_speech_prob > 0.85 {
        return true;
    }
    if known && (no_speech_prob > 0.3 || speech_ratio < 0.5) {
        return true;
    }
    // Credits-style text is never real speech in a live conversation.
    normalized.contains("amara.org")
}

/// Collapse a phrase repeated three or more times in a row ("I'm here. I'm
/// here. I'm here.") to a single occurrence. Single words need five
/// repetitions, so emphatic speech ("no, no, no!") is left alone.
pub fn collapse_repeats(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').filter(|w| !w.is_empty()).collect();
    if words.len() < 6 {
        return words.join(" ");
    }
    let mut out: Vec<&str> = Vec::with_capacity(words.len());
    let mut i = 0;
    'outer: while i < words.len() {
        for n in 1..=8 {
            let min_reps = if n == 1 { 5 } else { 3 };
            if i + n * min_reps > words.len() {
                continue;
            }
            let unit = &words[i..i + n];
            let mut reps = 1;
            while i + (reps + 1) * n <= words.len()
                && same(&words[i + reps * n..i + (reps + 1) * n], unit)
            {
                reps += 1;
            }
            if reps >= min_reps {
                out.extend_from_slice(unit);
                i += reps * n;
                continue 'outer;
            }
        }
        out.push(words[i]);
        i += 1;
    }
    out.join(" ")
}

fn same(a: &[&str], b: &[&str]) -> bool {
    let norm = |w: &str| {
        w.trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase()
    };
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| norm(x) == norm(y))
}

/// Join a running transcript prompt for the recogniser, keeping only the
/// last `max_chars` characters on a word boundary.
pub fn tail_for_prompt(text: &str, max_chars: usize) -> &str {
    if text.len() <= max_chars {
        return text;
    }
    let mut start = text.len() - max_chars;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    match text[start..].find(' ') {
        Some(space) => &text[start + space + 1..],
        None => &text[start..],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_strips_markers_and_whitespace() {
        assert_eq!(clean("  Hello\n  world  "), Some("Hello world".into()));
        assert_eq!(clean("[BLANK_AUDIO]"), None);
        assert_eq!(clean(" - "), None);
        assert_eq!(clean("[Music]"), Some("[Music]".into()));
        assert_eq!(clean("(applause)"), Some("(applause)".into()));
        assert_eq!(clean("¿Qué tal?"), Some("¿Qué tal?".into()));
    }

    #[test]
    fn hallucinations_need_supporting_evidence() {
        assert!(is_probable_hallucination("Thank you.", 0.5, 0.9));
        assert!(is_probable_hallucination("Thanks for watching!", 0.1, 0.2));
        // Someone really said thank you, clearly.
        assert!(!is_probable_hallucination("Thank you.", 0.05, 0.9));
        assert!(!is_probable_hallucination(
            "The results look good.",
            0.2,
            0.8
        ));
        assert!(is_probable_hallucination("Anything at all", 0.9, 0.9));
        assert!(is_probable_hallucination(
            "Subtitles by the Amara.org community",
            0.0,
            1.0
        ));
    }

    #[test]
    fn repeated_phrases_collapse() {
        assert_eq!(
            collapse_repeats("I'm here. I'm here. I'm here. I'm here. Okay then"),
            "I'm here. Okay then"
        );
        assert_eq!(collapse_repeats("no no no no no no"), "no");
        assert_eq!(
            collapse_repeats("no no no, stop it now please"),
            "no no no, stop it now please"
        );
        // Two repetitions can be genuine ("very very") and are kept.
        assert_eq!(
            collapse_repeats("it was very very good indeed my friend"),
            "it was very very good indeed my friend"
        );
    }

    #[test]
    fn prompt_tail_respects_word_boundaries() {
        assert_eq!(tail_for_prompt("short", 100), "short");
        assert_eq!(tail_for_prompt("alpha beta gamma delta", 11), "delta");
        assert_eq!(tail_for_prompt("naïve café crème brûlée", 12), "brûlée");
    }
}
