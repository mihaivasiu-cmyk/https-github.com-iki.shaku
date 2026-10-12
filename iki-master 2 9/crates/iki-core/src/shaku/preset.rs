//! Patch files: a small human-readable text format (`name = value` lines) over
//! the parameter tables, plus the factory patches. Free of any controller
//! types so it can be tested on its own.

pub const HEADER: &str = "iki-preset 1";

/// Parameters that are performance gestures, not part of a patch.
const SKIP: [&str; 4] = ["P_BEND", "P_FOLI_ON", "P_MERI", "P_IKI"];

/// `P_FILT_CUT` → `filt_cut`.
pub fn key(name: &str) -> String {
    name.strip_prefix("P_").unwrap_or(name).to_ascii_lowercase()
}

/// Render a patch. `groups` are the parameter tables (name, id); `get` reads a value.
pub fn to_text(name: &str, groups: &[&[(&str, i32)]], get: impl Fn(i32) -> f64) -> String {
    let mut s = format!("{HEADER}\nname = {}\n", name.replace('\n', " "));
    for g in groups {
        for (n, id) in g.iter() {
            if SKIP.contains(n) {
                continue;
            }
            s.push_str(&format!("{} = {:.5}\n", key(n), get(*id)));
        }
    }
    s
}

/// Parse a patch into (name, [(id, value)]). Unknown keys are ignored so older
/// and newer builds can share files; a wrong header is an error.
pub fn parse(text: &str, groups: &[&[(&str, i32)]]) -> Result<(String, Vec<(i32, f64)>), String> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
    if lines.next() != Some(HEADER) {
        return Err("not an Iki preset".into());
    }
    let mut name = String::new();
    let mut out = Vec::new();
    for l in lines {
        let Some((k, v)) = l.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        if k == "name" {
            name = v.to_string();
            continue;
        }
        let Ok(x) = v.parse::<f64>() else { continue };
        if !x.is_finite() {
            continue;
        }
        if let Some((_, id)) = groups.iter().flat_map(|g| g.iter()).find(|(n, _)| !SKIP.contains(n) && key(n) == k) {
            out.push((*id, x));
        }
    }
    Ok((name, out))
}

/// Factory patches: (name, overrides). Anything not listed keeps its default.
pub const FACTORY: &[(&str, &[(&str, f64)])] = &[
    ("Init", &[]),
    (
        "Temple",
        &[
            ("reverb", 0.55),
            ("reverb_decay", 0.78),
            ("yuri", 0.35),
            ("glide", 0.3),
            ("flute_level", 0.85),
            ("bore_level", 0.0),
        ],
    ),
    ("Breathy bore", &[("bore_level", 0.9), ("flute_level", 0.0), ("yuri", 0.2), ("filt_cut", 0.535), ("reverb", 0.3)]),
    (
        "Both, wide",
        &[
            ("flute_level", 0.7),
            ("bore_level", 0.6),
            ("width", 0.5),
            ("reverb", 0.4),
            ("yuri", 0.25),
            ("delay_mix", 0.18),
        ],
    ),
    ("Dry solo", &[("reverb", 0.0), ("yuri", 0.15), ("glide", 0.08)]),
];

/// A factory patch as parse-able text (so loading it is the same path as a file).
pub fn factory_text(i: usize, groups: &[&[(&str, i32)]]) -> Option<String> {
    let (name, set) = FACTORY.get(i)?;
    let mut s = format!("{HEADER}\nname = {name}\n");
    for (k, v) in set.iter() {
        if groups.iter().flat_map(|g| g.iter()).any(|(n, _)| key(n) == *k) {
            s.push_str(&format!("{k} = {v:.5}\n"));
        }
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &[(&str, i32)] = &[("P_GAIN", 3), ("P_MERI", 0), ("P_BEND", 33), ("P_YURI", 35)];
    const B: &[(&str, i32)] = &[("P_VOICE", 67), ("P_FOLI_ON", 49)];

    #[test]
    fn roundtrips_and_skips_gestures() {
        let t = to_text("My patch", &[A, B], |id| id as f64 / 100.0);
        assert!(t.contains("gain = 0.03000") && t.contains("voice = 0.67000"));
        assert!(!t.contains("meri") && !t.contains("bend") && !t.contains("foli_on"));
        let (name, v) = parse(&t, &[A, B]).unwrap();
        assert_eq!(name, "My patch");
        assert_eq!(v.len(), 3);
        assert!(v.contains(&(35, 0.35)));
    }

    #[test]
    fn tolerates_junk_and_rejects_foreign_files() {
        assert!(parse("hello", &[A]).is_err());
        let (_, v) = parse("iki-preset 1\n# c\nfuture = 1\ngain = x\nyuri = 0.5\nnonsense\n", &[A]).unwrap();
        assert_eq!(v, vec![(35, 0.5)]);
    }

    #[test]
    fn factory_patches_parse() {
        let all: &[&[(&str, i32)]] = &[&[("P_REVERB", 8), ("P_YURI", 35), ("P_GLIDE", 37)]];
        for i in 0..FACTORY.len() {
            let t = factory_text(i, all).unwrap();
            assert!(parse(&t, all).is_ok());
        }
        assert!(factory_text(FACTORY.len(), all).is_none());
    }
}
