import Foundation

/// Post-processes a raw Parakeet transcript before it's pasted into the
/// focused app. Passes:
///   1. Strip filler words (um, uh, øh, …) per detected language.
///   2. Apply user substitutions (e.g. "open whisper" → "OpenWhisper").
///
/// Filler lists are keyed by `FillerLang`. Critical distinction:
/// "er" is English hesitation ("er, I think…") but also the Danish
/// copula ("det er fedt"). Stripping it blindly ate Danish verbs — hence
/// per-language registers + lightweight detection.
///
/// Lives on the @MainActor for consistency with the rest of the service
/// layer, but the transformation itself is pure and cheap — safe to call
/// from a transcription-completion handler.
@MainActor
final class TranscriptProcessor {
    enum FillerLang: String, Sendable {
        case en
        case da
    }

    /// Per-language filler register. Each list is self-contained so a
    /// reviewer can scan what gets stripped for a given language without
    /// mentally intersecting sets.
    ///
    /// Note the asymmetry around "er": present in `.en` (hesitation),
    /// absent from `.da` (copula "is/are"). "erm"/"err"/"errm" are in both
    /// — they're never Danish words, so safe to strip as standalone tokens.
    static let defaultFillersByLang: [FillerLang: Set<String>] = [
        .en: [
            "um", "umm", "ummm",
            "uh", "uhh", "uhhh",
            "uhm", "uhmm",
            "er", "err", "erm", "errm",
            "ah", "ahh",
            "oh", "ooh",
            "hm", "hmm", "hmmm",
            "mm", "mmm",
        ],
        .da: [
            "um", "umm", "ummm",
            "uh", "uhh", "uhhh",
            "uhm", "uhmm",
            "err", "erm", "errm",
            "ah", "ahh",
            "oh", "ooh",
            "hm", "hmm", "hmmm",
            "mm", "mmm",
            "øh", "øhm",
        ],
    ]

    /// Default substitutions. Kept short — users will add their own via
    /// settings once TASK-11 lands.
    static let defaultSubstitutions: [String: String] = [
        "open whisper": "OpenWhisper",
    ]

    var removeFillers: Bool = true
    var fillersByLang: [FillerLang: Set<String>] = TranscriptProcessor.defaultFillersByLang
    var substitutions: [String: String] = TranscriptProcessor.defaultSubstitutions

    /// Transcription language to apply. Nil = auto-detect from text.
    /// Will be wired to FluidAudio's detected language once issue #303 ships.
    func process(_ text: String, lang: FillerLang? = nil) -> String {
        let effectiveLang = lang ?? Self.detectLang(text)
        var out = text
        if removeFillers {
            out = strippingFillers(out, lang: effectiveLang)
        }
        out = applyingSubstitutions(out)
        out = normalizingWhitespace(out)
        return out
    }

    /// Heuristic language detection from text. Today: any Danish-specific
    /// character (æ/ø/å) flips to .da; otherwise .en. Intentionally simple
    /// — replace with FluidAudio's per-result language once available.
    static func detectLang(_ text: String) -> FillerLang {
        let daChars: Set<Character> = ["æ", "ø", "å", "Æ", "Ø", "Å"]
        return text.contains(where: daChars.contains) ? .da : .en
    }

    // MARK: - Passes

    private func strippingFillers(_ text: String, lang: FillerLang) -> String {
        guard let fillers = fillersByLang[lang], !fillers.isEmpty else {
            return text
        }

        // Match: word-boundary, one of the fillers, word-boundary, then
        // optionally a single trailing comma (but NOT a period — removing a
        // period eats sentence boundaries), then any trailing whitespace.
        let alternatives = fillers
            .sorted(by: { $0.count > $1.count }) // longer variants first
            .map { NSRegularExpression.escapedPattern(for: $0) }
            .joined(separator: "|")
        let pattern = "\\b(?:\(alternatives))\\b,?\\s*"

        guard let regex = try? NSRegularExpression(
            pattern: pattern,
            options: [.caseInsensitive]
        ) else {
            return text
        }

        let ns = text as NSString
        return regex.stringByReplacingMatches(
            in: text,
            options: [],
            range: NSRange(location: 0, length: ns.length),
            withTemplate: ""
        )
    }

    private func applyingSubstitutions(_ text: String) -> String {
        guard !substitutions.isEmpty else { return text }
        var out = text
        // Longest keys first so "open whisper" wins over a hypothetical "open".
        let ordered = substitutions.sorted { $0.key.count > $1.key.count }
        for (key, value) in ordered {
            let escapedKey = NSRegularExpression.escapedPattern(for: key)
            let pattern = "\\b\(escapedKey)\\b"
            out = out.replacingOccurrences(
                of: pattern,
                with: value,
                options: [.regularExpression, .caseInsensitive]
            )
        }
        return out
    }

    private func normalizingWhitespace(_ text: String) -> String {
        var out = text
        // Collapse runs of spaces/tabs into a single space.
        out = out.replacingOccurrences(
            of: "[ \\t]{2,}",
            with: " ",
            options: .regularExpression
        )
        // Remove whitespace that got left stranded in front of punctuation.
        out = out.replacingOccurrences(
            of: " ([,.!?;:])",
            with: "$1",
            options: .regularExpression
        )
        // Collapse ", , ," → "," which can happen when we strip "um, uh,".
        out = out.replacingOccurrences(
            of: "(,\\s*){2,}",
            with: ", ",
            options: .regularExpression
        )
        return out.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
