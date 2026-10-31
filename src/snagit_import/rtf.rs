//! Small bounded RTF text reader. Formatting is mapped from the object metadata;
//! font/color tables and ignorable destinations are skipped. Visible inline media
//! and fields are rejected instead of being silently lost.
pub(super) fn plain_text(bytes: &[u8]) -> Result<String, String> {
    if !bytes.starts_with(b"{\\rtf") {
        return Err("Snagit text is not valid RTF.".into());
    }
    #[derive(Clone, Copy)]
    struct State {
        skip: bool,
        unicode_skip: usize,
        codepage: u16,
    }
    let mut state = State {
        skip: false,
        unicode_skip: 1,
        codepage: 1252,
    };
    let mut stack = Vec::new();
    let mut out = String::new();
    let mut i = 0;
    let mut fallback = 0;
    let mut pending_high = None;
    while i < bytes.len() {
        let b = bytes[i];
        i += 1;
        match b {
            b'{' => {
                if stack.len() >= 128 {
                    return Err("Snagit text nesting is too deep.".into());
                }
                stack.push(state);
            }
            b'}' => {
                state = stack.pop().ok_or("Unbalanced Snagit rich text.")?;
            }
            b'\\' => {
                let Some(&next) = bytes.get(i) else {
                    return Err("Incomplete Snagit rich text escape.".into());
                };
                i += 1;
                match next {
                    b'\\' | b'{' | b'}' => {
                        if fallback > 0 {
                            fallback -= 1;
                        } else if !state.skip {
                            out.push(next as char);
                        }
                    }
                    b'*' => state.skip = true,
                    b'~' | b'_' | b'-' => {
                        if fallback > 0 {
                            fallback -= 1;
                        } else if !state.skip && next != b'-' {
                            out.push(if next == b'~' { '\u{a0}' } else { '\u{2011}' });
                        }
                    }
                    b'\'' => {
                        let hex = bytes.get(i..i + 2).ok_or("Incomplete Snagit text byte.")?;
                        i += 2;
                        let text =
                            std::str::from_utf8(hex).map_err(|_| "Invalid Snagit text byte.")?;
                        let value = u8::from_str_radix(text, 16)
                            .map_err(|_| "Invalid Snagit text byte.")?;
                        if fallback > 0 {
                            fallback -= 1;
                        } else if !state.skip {
                            out.push(decode_byte(value, state.codepage));
                        }
                    }
                    value if value.is_ascii_alphabetic() => {
                        let begin = i - 1;
                        while bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
                            i += 1;
                        }
                        let word = &bytes[begin..i];
                        let number_start = i;
                        if bytes.get(i) == Some(&b'-') {
                            i += 1;
                        }
                        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                            i += 1;
                        }
                        let number = if i > number_start {
                            std::str::from_utf8(&bytes[number_start..i])
                                .ok()
                                .and_then(|s| s.parse::<i32>().ok())
                        } else {
                            None
                        };
                        if bytes.get(i) == Some(&b' ') {
                            i += 1;
                        }
                        match word {
                            b"ansicpg" => {
                                state.codepage=match number {
                                    Some(1252)=>1252,
                                    Some(1254)=>1254,
                                    _=>return Err("This Snagit rich-text character encoding is not supported yet. Use Unicode or Windows-1252/1254 text in Snagit.".into()),
                                };
                            }

                            b"fonttbl" | b"colortbl" | b"stylesheet" | b"info"
                            | b"header" | b"footer" | b"generator"
                            | b"filetbl" | b"listtable" | b"listoverridetable" => state.skip = true,
                            b"pict" | b"object" | b"field" if !state.skip => return Err("Inline Snagit rich-text images, embedded objects and dynamic fields are not supported yet; no content was discarded.".into()),
                            b"mac" | b"pc" | b"pca" => return Err(
                                "This Snagit rich-text character encoding is not supported yet."
                                    .into(),
                            ),
                            b"uc" => {
                                state.unicode_skip = number
                                    .filter(|n| (0..=16).contains(n))
                                    .ok_or("Invalid Snagit Unicode fallback length.")?
                                    as usize;
                            }
                            b"u" if !state.skip => {
                                let unit = number
                                    .filter(|n| (-32768..=65535).contains(n))
                                    .ok_or("Invalid Snagit Unicode character.")?
                                    as u16;
                                if (0xd800..=0xdbff).contains(&unit) {
                                    if pending_high.replace(unit).is_some() {
                                        return Err("Invalid Snagit Unicode pair.".into());
                                    }
                                } else if (0xdc00..=0xdfff).contains(&unit) {
                                    let high = pending_high
                                        .take()
                                        .ok_or("Invalid Snagit Unicode pair.")?;
                                    let cp = 0x10000
                                        + (((u32::from(high) - 0xd800) << 10)
                                            | (u32::from(unit) - 0xdc00));
                                    out.push(
                                        char::from_u32(cp)
                                            .ok_or("Invalid Snagit Unicode character.")?,
                                    );
                                } else {
                                    if pending_high.is_some() {
                                        return Err("Invalid Snagit Unicode pair.".into());
                                    }
                                    out.push(
                                        char::from_u32(u32::from(unit))
                                            .ok_or("Invalid Snagit Unicode character.")?,
                                    );
                                }
                                fallback = state.unicode_skip;
                            }
                            b"par" | b"line" if !state.skip => out.push('\n'),
                            b"tab" if !state.skip => out.push('\t'),
                            b"emdash" if !state.skip => out.push('\u{2014}'),
                            b"endash" if !state.skip => out.push('\u{2013}'),
                            b"bullet" if !state.skip => out.push('\u{2022}'),
                            b"lquote" if !state.skip => out.push('\u{2018}'),
                            b"rquote" if !state.skip => out.push('\u{2019}'),
                            b"ldblquote" if !state.skip => out.push('\u{201c}'),
                            b"rdblquote" if !state.skip => out.push('\u{201d}'),
                            b"bin" => {
                                let count = number
                                    .filter(|n| *n >= 0)
                                    .ok_or("Invalid binary RTF payload.")?
                                    as usize;
                                i = i
                                    .checked_add(count)
                                    .filter(|n| *n <= bytes.len())
                                    .ok_or("Truncated RTF binary payload.")?;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            b'\n' | b'\r' => {}
            _ => {
                if fallback > 0 {
                    fallback -= 1;
                } else if !state.skip {
                    out.push(decode_byte(b, state.codepage));
                }
            }
        }
        if out.len() > 64 * 1024 {
            return Err("Imported Snagit text exceeds 64 KB.".into());
        }
    }
    if !stack.is_empty() || pending_high.is_some() {
        return Err("Incomplete Snagit rich text.".into());
    }
    Ok(out)
}
fn decode_byte(b: u8, codepage: u16) -> char {
    if codepage == 1254 {
        match b {
            0xd0 => 'Ğ',
            0xdd => 'İ',
            0xde => 'Ş',
            0xf0 => 'ğ',
            0xfd => 'ı',
            0xfe => 'ş',
            _ => cp1252(b),
        }
    } else {
        cp1252(b)
    }
}

fn cp1252(b: u8) -> char {
    const EXTRA: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    if (0x80..=0x9f).contains(&b) {
        EXTRA[(b - 0x80) as usize]
    } else {
        b as char
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_escaped_tables_and_paragraphs() {
        let r=br"{\rtf1\ansi{\fonttbl{\f0 Arial;}}{\colortbl;\red255\green0\blue0;}\f0 Hello \u287?\u252?zel\par \{text\} \'e7 \u-10179?\u-8704?}";
        assert_eq!(plain_text(r).unwrap(), "Hello ğüzel\n{text} ç 😀");
    }
    #[test]
    fn turkish_codepage_and_unicode_fallback_symbols_are_preserved() {
        assert_eq!(
            plain_text(br"{\rtf1\ansi\ansicpg1254 \'d0\'dd\'de\'f0\'fd\'fe}").unwrap(),
            "ĞİŞğış"
        );
        assert_eq!(plain_text(br"{\rtf1\ansi\uc1\u287\~x}").unwrap(), "ğx");
        assert_eq!(plain_text(br"{\rtf1\ansi\uc1\u287\_x}").unwrap(), "ğx");
        assert_eq!(plain_text(br"{\rtf1\ansi\uc1\u287\-x}").unwrap(), "ğx");
        assert!(plain_text(br"{\rtf1\ansi\ansicpg932 text}").is_err());
    }
    #[test]
    fn rejects_invalid_rtf_and_skips_binary_destinations() {
        assert!(plain_text(b"plain").is_err());
        for bytes in [
            br"{\rtf1 A{\pict image}B}".as_slice(),
            br"{\rtf1 A{\object embedded}B}".as_slice(),
            br"{\rtf1 A{\field dynamic}B}".as_slice(),
        ] {
            assert!(plain_text(bytes).is_err());
        }
        assert!(plain_text(br"{\rtf1 broken").is_err());
        assert_eq!(
            plain_text(br"{\rtf1 A{\*\unknown secret}B{\*\pict\bin3 {xx}C}").unwrap(),
            "ABC"
        );
    }
}
