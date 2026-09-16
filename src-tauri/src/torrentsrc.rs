use percent_encoding::percent_decode_str;

#[derive(Debug, Default, Clone)]
pub struct ParsedMagnet {
    pub info_hash: Option<String>,
    pub name: Option<String>,
    pub trackers: Vec<String>,
    pub size: Option<u64>,
}

/// Parses the parts of a magnet URI we show in the add sheet. Deliberately
/// lenient: anything we cannot read is simply left as None.
pub fn parse_magnet(uri: &str) -> ParsedMagnet {
    let mut out = ParsedMagnet::default();
    let query = match uri.split_once('?') {
        Some((_, q)) => q,
        None => return out,
    };
    for pair in query.split('&') {
        let (key, value) = match pair.split_once('=') {
            Some(kv) => kv,
            None => continue,
        };
        let value = percent_decode_str(value).decode_utf8_lossy().into_owned();
        match key {
            "xt" => {
                if let Some(hash) = value.strip_prefix("urn:btih:") {
                    out.info_hash = Some(normalize_info_hash(hash));
                } else if let Some(hash) = value.strip_prefix("urn:btmh:") {
                    out.info_hash = Some(hash.to_ascii_lowercase());
                }
            }
            "dn" => out.name = Some(value.replace('+', " ")),
            "tr" => {
                let t = value.trim().to_string();
                if !t.is_empty() {
                    out.trackers.push(t);
                }
            }
            "xl" => out.size = value.parse().ok(),
            _ => {}
        }
    }
    out
}

fn normalize_info_hash(hash: &str) -> String {
    let h = hash.trim();
    if h.len() == 32 {
        // base32 encoded info hash
        base32_to_hex(h).unwrap_or_else(|| h.to_ascii_lowercase())
    } else {
        h.to_ascii_lowercase()
    }
}

fn base32_to_hex(input: &str) -> Option<String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bits = 0u32;
    let mut nbits = 0u32;
    let mut bytes = Vec::with_capacity(20);
    for c in input.trim_end_matches('=').bytes() {
        let upper = c.to_ascii_uppercase();
        let idx = ALPHABET.iter().position(|a| *a == upper)? as u32;
        bits = (bits << 5) | idx;
        nbits += 5;
        if nbits >= 8 {
            nbits -= 8;
            bytes.push(((bits >> nbits) & 0xff) as u8);
        }
    }
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Splits a pasted blob into individual tracker URLs. Accepts newline, comma
/// and whitespace separated lists, which is how tracker lists are shared.
pub fn parse_tracker_blob(blob: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    blob.split(|c: char| c == '\n' || c == '\r' || c == ',' || c == ';' || c.is_whitespace())
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.starts_with("http://")
                || lower.starts_with("https://")
                || lower.starts_with("udp://")
                || lower.starts_with("ws://")
                || lower.starts_with("wss://")
        })
        .filter(|line| seen.insert(line.to_ascii_lowercase()))
        .map(str::to_string)
        .collect()
}

/// Merges tracker lists, keeping the first occurrence of each URL.
pub fn merge_trackers(lists: &[&[String]]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for list in lists {
        for t in list.iter() {
            let t = t.trim();
            if !t.is_empty() && seen.insert(t.to_ascii_lowercase()) {
                out.push(t.to_string());
            }
        }
    }
    out
}

/// Is this string something we can hand to the engine as a torrent source?
pub fn looks_like_source(s: &str) -> bool {
    let s = s.trim();
    let lower = s.to_ascii_lowercase();
    lower.starts_with("magnet:")
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || (s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_magnet_fields() {
        let m = parse_magnet(
            "magnet:?xt=urn:btih:CAB507494D02EBB1178B38F2E9D7BE299C86B862&dn=Big+Buck+Bunny\
             &tr=udp%3A%2F%2Ftracker.example%3A1337%2Fannounce",
        );
        assert_eq!(
            m.info_hash.as_deref(),
            Some("cab507494d02ebb1178b38f2e9d7be299c86b862")
        );
        assert_eq!(m.name.as_deref(), Some("Big Buck Bunny"));
        assert_eq!(m.trackers, vec!["udp://tracker.example:1337/announce"]);
    }

    #[test]
    fn tracker_blob_filters_and_dedupes() {
        let list = parse_tracker_blob(
            "udp://a:1/announce\n\n# comment\nudp://A:1/announce\nnot-a-url\nhttps://b/announce",
        );
        assert_eq!(list, vec!["udp://a:1/announce", "https://b/announce"]);
    }

    #[test]
    fn decodes_base32_info_hash() {
        // Same hash as the hex test, base32 encoded.
        let m = parse_magnet("magnet:?xt=urn:btih:ZK2QOSKNALV3CF4LHDZOTV56FGOINODC");
        assert_eq!(
            m.info_hash.as_deref(),
            Some("cab507494d02ebb1178b38f2e9d7be299c86b862")
        );
    }
}
