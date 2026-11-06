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

/// The trackers a torrent file announces to, in order, without repeats.
pub fn announced(torrent: &[u8]) -> Vec<String> {
    let Ok(meta) = librqbit::torrent_from_bytes(torrent) else {
        return Vec::new();
    };
    let list: Vec<String> = meta
        .iter_announce()
        .filter_map(|t| std::str::from_utf8(t.as_ref()).ok().map(str::to_string))
        .collect();
    merge_trackers(&[&list])
}

/// The .torrent with its announce list replaced by `trackers`, one per tier.
/// Everything else, the info dictionary included, is copied byte for byte so
/// the info hash stays the same. None when the bytes are not a torrent file.
pub fn with_trackers(torrent: &[u8], trackers: &[String]) -> Option<Vec<u8>> {
    let mut entries: Vec<(&[u8], Vec<u8>)> = top_level_entries(torrent)?
        .into_iter()
        .filter(|(key, _)| *key != b"announce" && *key != b"announce-list")
        .map(|(key, value)| (key, value.to_vec()))
        .collect();
    if let Some(first) = trackers.first() {
        entries.push((b"announce", bencode_string(first.as_bytes())));
        let mut tiers = b"l".to_vec();
        for t in trackers {
            tiers.push(b'l');
            tiers.extend(bencode_string(t.as_bytes()));
            tiers.push(b'e');
        }
        tiers.push(b'e');
        entries.push((b"announce-list", tiers));
    }
    entries.sort_by(|a, b| a.0.cmp(b.0));

    let mut out = b"d".to_vec();
    for (key, value) in entries {
        out.extend(bencode_string(key));
        out.extend(value);
    }
    out.push(b'e');
    Some(out)
}

fn bencode_string(s: &[u8]) -> Vec<u8> {
    let mut out = format!("{}:", s.len()).into_bytes();
    out.extend_from_slice(s);
    out
}

/// The keys of a bencoded dictionary with the raw bytes of each value.
fn top_level_entries(data: &[u8]) -> Option<Vec<(&[u8], &[u8])>> {
    if data.first() != Some(&b'd') {
        return None;
    }
    let mut pos = 1;
    let mut entries = Vec::new();
    while *data.get(pos)? != b'e' {
        let (key, after_key) = bencode_bytes(data, pos)?;
        let end = skip_value(data, after_key, 0)?;
        entries.push((key, &data[after_key..end]));
        pos = end;
    }
    (pos + 1 == data.len()).then_some(entries)
}

/// A byte string starting at `pos`, and where it ends.
fn bencode_bytes(data: &[u8], pos: usize) -> Option<(&[u8], usize)> {
    let colon = pos + data.get(pos..)?.iter().position(|&b| b == b':')?;
    let len: usize = std::str::from_utf8(&data[pos..colon]).ok()?.parse().ok()?;
    let start = colon + 1;
    let end = start.checked_add(len)?;
    Some((data.get(start..end)?, end))
}

/// Where the bencoded value starting at `pos` ends.
fn skip_value(data: &[u8], pos: usize, depth: usize) -> Option<usize> {
    if depth > 64 {
        return None;
    }
    match *data.get(pos)? {
        b'i' => Some(pos + data.get(pos..)?.iter().position(|&b| b == b'e')? + 1),
        b'0'..=b'9' => bencode_bytes(data, pos).map(|(_, end)| end),
        b'l' => {
            let mut pos = pos + 1;
            while *data.get(pos)? != b'e' {
                pos = skip_value(data, pos, depth + 1)?;
            }
            Some(pos + 1)
        }
        b'd' => {
            let mut pos = pos + 1;
            while *data.get(pos)? != b'e' {
                let (_, after_key) = bencode_bytes(data, pos)?;
                pos = skip_value(data, after_key, depth + 1)?;
            }
            Some(pos + 1)
        }
        _ => None,
    }
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
    fn trackers_are_replaced_without_touching_the_info_hash() {
        let original = b"d8:announce18:udp://a:1/announce13:announce-listll18:udp://a:1/announceee\
            7:comment2:hi4:infod6:lengthi1e4:name1:a12:piece lengthi16384e\
            6:pieces20:AAAAAAAAAAAAAAAAAAAAee";
        let hash = |bytes: &[u8]| librqbit::torrent_from_bytes(bytes).unwrap().info_hash;
        let announced = |bytes: &[u8]| -> Vec<String> {
            librqbit::torrent_from_bytes(bytes)
                .unwrap()
                .iter_announce()
                .map(|t| String::from_utf8_lossy(t.as_ref()).into_owned())
                .collect()
        };

        let wanted = vec!["udp://b:2/announce".to_string(), "https://c/announce".to_string()];
        let rewritten = with_trackers(original, &wanted).unwrap();
        assert_eq!(hash(&rewritten), hash(original));
        assert!(!announced(&rewritten).contains(&"udp://a:1/announce".to_string()));
        assert!(announced(&rewritten).contains(&"https://c/announce".to_string()));
        assert!(rewritten.windows(9).any(|w| w == b"7:comment"));

        let bare = with_trackers(original, &[]).unwrap();
        assert_eq!(hash(&bare), hash(original));
        assert!(announced(&bare).is_empty());

        assert_eq!(with_trackers(b"not a torrent", &wanted), None);
        assert_eq!(with_trackers(b"d4:infoi1e", &wanted), None);
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
