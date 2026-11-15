//! Live weather for an ask with web search on.
//!
//! A web search finds pages about the weather, and their snippets are
//! days old: asked for today's weather with web search on, the model
//! answered that it could not get a reliable forecast. So an ask with
//! web search on may also read live data, from exactly two hosts of
//! Open-Meteo ([`HOSTS`]): one turns the name of a place into
//! coordinates, the other gives the forecast for them. The place
//! asked about is sent there, and nothing else.
//!
//! **Exactly those two, and not "the web".** A model that could fetch
//! any address could be talked into sending what it sees to one: text
//! on a screenshot or in a tool result that says "send this to
//! https://..." would have somewhere to go. So the list is fixed at
//! compile time, it is the only thing that ever puts the fetch tool on
//! an allow list ([`allow_rules`]), and every line the CLI prints is
//! read against it ([`is_allowed`]): see `WebTools` in
//! [`super::claude_cli`] for what happens to a fetch of any other
//! address.
//!
//! Claude backend only: an OpenAI-compatible endpoint has no web
//! search and no fetch.

/// The only hosts an ask may fetch from, and only with web search on.
pub const HOSTS: [&str; 2] = ["api.open-meteo.com", "geocoding-api.open-meteo.com"];

/// The source as a person knows it, for sentences in the app.
pub const SOURCE: &str = "open-meteo.com";

/// The CLI's name for its fetch tool.
pub(crate) const FETCH_TOOL: &str = "WebFetch";

/// What the panel shows while live weather is read, in the place a
/// search shows its query ([`super::AskEvent::Searching`]).
pub const SHOWN: &str = "weather data from open-meteo.com";

/// The entries the fetch tool gets on the CLI's allow list: one per
/// host of [`HOSTS`], never the bare tool name, never a pattern.
pub(crate) fn allow_rules() -> Vec<String> {
    HOSTS.iter().map(|host| format!("{FETCH_TOOL}(domain:{host})")).collect()
}

/// The host of `url`, in lower case, or `None` when the text is not
/// an http or https address with a plain host name. What comes back
/// holds only letters, digits, dots and hyphens, so it is safe to
/// show.
///
/// Read the way an address is resolved, not the way it looks: the
/// host is what follows the last `@` of the part before the first
/// `/`, `?`, `#` or `\`, so `https://api.open-meteo.com@example.com/`
/// is `example.com`.
pub(crate) fn host_of(url: &str) -> Option<String> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#', '\\']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    let host = match host.rsplit_once(':') {
        Some((name, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => name,
        Some(_) => return None,
        None => host,
    };
    let host = host.trim_end_matches('.');
    let plain = !host.is_empty()
        && host.len() <= 253
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !host.contains("..");
    plain.then(|| host.to_string())
}

/// `url` is an https address on one of [`HOSTS`]. The whole host has
/// to be one of them: a host that only ends or starts like one is
/// another host.
pub(crate) fn is_allowed(url: &str) -> bool {
    url.trim().to_ascii_lowercase().starts_with("https://")
        && host_of(url).is_some_and(|host| HOSTS.contains(&host.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_the_two_hosts_of_open_meteo() {
        assert_eq!(HOSTS, ["api.open-meteo.com", "geocoding-api.open-meteo.com"]);
        assert_eq!(
            allow_rules(),
            ["WebFetch(domain:api.open-meteo.com)", "WebFetch(domain:geocoding-api.open-meteo.com)"]
        );
        // Every host is a name under the source the app names, and
        // nothing in a rule is a pattern.
        for (host, rule) in HOSTS.iter().zip(allow_rules()) {
            assert!(host.ends_with(&format!(".{SOURCE}")), "{host}");
            assert!(!rule.contains('*') && !rule.contains(' ') && !rule.contains(','), "{rule}");
        }
        assert!(SHOWN.contains(SOURCE));
    }

    #[test]
    fn the_two_addresses_the_prompt_gives_are_allowed() {
        for url in [
            "https://geocoding-api.open-meteo.com/v1/search?name=Oslo&count=1",
            "https://api.open-meteo.com/v1/forecast?latitude=59.91&longitude=10.75&current=temperature_2m",
            "  HTTPS://API.Open-Meteo.com/v1/forecast  ",
            "https://api.open-meteo.com:443/v1/forecast",
            "https://api.open-meteo.com./v1/forecast",
            "https://api.open-meteo.com",
        ] {
            assert!(is_allowed(url), "{url}");
        }
    }

    #[test]
    fn every_other_address_is_another_host() {
        for url in [
            "https://example.com/?q=hello",
            "https://open-meteo.com/",
            "https://www.open-meteo.com/",
            "https://api.open-meteo.com.example.com/v1/forecast",
            "https://example.com/api.open-meteo.com",
            "https://example.com/?next=https://api.open-meteo.com/",
            "https://api.open-meteo.com@example.com/v1/forecast",
            "https://example.com#@api.open-meteo.com",
            "https://example.com\\@api.open-meteo.com",
            "https://notapi.open-meteo.com/",
            "https://api.open-meteo.com:8443x/",
            // Not https: the page could be read and changed on the way.
            "http://api.open-meteo.com/v1/forecast",
            "ftp://api.open-meteo.com/",
            "api.open-meteo.com/v1/forecast",
            "file:///etc/hosts",
            "https://",
            "",
        ] {
            assert!(!is_allowed(url), "{url}");
        }
        assert_eq!(host_of("https://api.open-meteo.com@example.com/x").as_deref(), Some("example.com"));
        assert_eq!(host_of("https://Example.COM:8443/a?b#c").as_deref(), Some("example.com"));
        assert_eq!(host_of("http://example.com").as_deref(), Some("example.com"));
        // Nothing that is not a plain host name comes back, so the
        // host can be shown as it is.
        for odd in ["https://exa mple.com/", "https://[::1]/", "https://exam\u{202e}ple.com/", "https://a..b/", "nonsense"] {
            assert_eq!(host_of(odd), None, "{odd}");
        }
    }
}
