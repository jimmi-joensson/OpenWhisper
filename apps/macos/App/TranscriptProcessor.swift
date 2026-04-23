import Foundation

/// Post-processes a raw Parakeet transcript before it's pasted into the
/// focused app. Two passes today:
///   1. Strip filler words (um, uh, er, ah, oh, …).
///   2. Apply user substitutions (e.g. "open whisper" → "OpenWhisper",
///      "payproof" → "PayProff").
///
/// Lives on the @MainActor for consistency with the rest of the service
/// layer, but the transformation itself is pure and cheap — safe to call
/// from a transcription-completion handler.
@MainActor
final class TranscriptProcessor {
    /// Default filler-word set. Case-insensitive whole-word matching, so
    /// "umbrella" and "oh-well" survive. Multi-letter repeats like "uhhh"
    /// and "ummmmm" are caught via the regex pattern, not this list.
    static let defaultFillers: Set<String> = [
        "um", "umm", "ummm",
        "uh", "uhh", "uhhh",
        "uhm", "uhmm",
        "er", "err", "erm", "errm",
        "ah", "ahh",
        "oh", "ooh",
        "hm", "hmm", "hmmm",
        "mm", "mmm",
        "øh", "øhm", // Danish
    ]

    /// Default substitutions. Kept short — users will add their own via
    /// settings once TASK-11 lands.
    static let defaultSubstitutions: [String: String] = [
        "open whisper": "OpenWhisper",
    ]

    var removeFillers: Bool = true
    var fillers: Set<String> = TranscriptProcessor.defaultFillers
    var substitutions: [String: String] = TranscriptProcessor.defaultSubstitutions

    func process(_ text: String) -> String {
        var out = text
        if removeFillers {
            out = strippingFillers(out)
        }
        out = applyingSubstitutions(out)
        out = normalizingWhitespace(out)
        return out
    }

    // MARK: - Passes

    private func strippingFillers(_ text: String) -> String {
        guard !fillers.isEmpty else { return text }

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
