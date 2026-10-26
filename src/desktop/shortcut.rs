//! Resolve capture shortcuts once during desktop initialization.
//!
//! macOS registers physical virtual keys, so the US `Code::Period` position is
//! not necessarily the key labeled '.' on the active layout. Resolve the
//! unmodified characters first, then register Control + Shift + C and the
//! Control/Command + Shift + period shortcuts with their respective keys.
//! Restart MyImage after changing keyboard layouts. Windows continues to use
//! global-hotkey's `Code::Period` / `Code::KeyC` mappings to `VK_OEM_PERIOD` / `VK_C`.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};

pub(super) struct CaptureHotkey {
    pub label: &'static str,
    pub hotkey: HotKey,
}

pub(super) struct CaptureHotkeys {
    pub hotkeys: Vec<CaptureHotkey>,
    pub errors: Vec<String>,
}

/// Call on the main UI thread; Apple's keyboard-layout APIs are not thread safe.
/// Resolve the two characters independently so an unavailable key does not
/// prevent the other shortcuts from registering.
pub(super) fn capture_hotkeys() -> CaptureHotkeys {
    #[cfg(target_os = "macos")]
    let keys = (macos::character_key(b'.'), macos::character_key(b'c'));
    #[cfg(not(target_os = "macos"))]
    let keys = (Ok(Code::Period), Ok(Code::KeyC));

    capture_hotkeys_for_keys(keys.0, keys.1, cfg!(target_os = "macos"))
}

fn capture_hotkeys_for_keys(
    period: Result<Code, String>,
    letter_c: Result<Code, String>,
    include_command: bool,
) -> CaptureHotkeys {
    let mut result = CaptureHotkeys {
        hotkeys: Vec::new(),
        errors: Vec::new(),
    };
    // CONTROL deliberately means Control on macOS, not Command.
    match letter_c {
        Ok(key) => result.hotkeys.push(CaptureHotkey {
            label: "Ctrl+Shift+C",
            hotkey: HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), key),
        }),
        Err(error) => result
            .errors
            .push(format!("Ctrl+Shift+C unavailable: {error}")),
    }
    match period {
        Ok(key) => {
            result.hotkeys.push(CaptureHotkey {
                label: "Ctrl+Shift+.",
                hotkey: HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), key),
            });
            if include_command {
                result.hotkeys.push(CaptureHotkey {
                    label: "Cmd+Shift+.",
                    hotkey: HotKey::new(Some(Modifiers::SUPER | Modifiers::SHIFT), key),
                });
            }
        }
        Err(error) => result
            .errors
            .push(format!("Period capture shortcuts unavailable: {error}")),
    }
    result
}

/// These are the main typing keys supported by global-hotkey 0.8's macOS
/// key_to_scancode table. Code names describe US positions, not layout text.
/// ISO/JIS-specific positions and the keypad are intentionally not substituted.
#[cfg(any(target_os = "macos", test))]
fn code_for_macos_key(virtual_key: u16) -> Option<Code> {
    Some(match virtual_key {
        0x00 => Code::KeyA,
        0x01 => Code::KeyS,
        0x02 => Code::KeyD,
        0x03 => Code::KeyF,
        0x04 => Code::KeyH,
        0x05 => Code::KeyG,
        0x06 => Code::KeyZ,
        0x07 => Code::KeyX,
        0x08 => Code::KeyC,
        0x09 => Code::KeyV,
        0x0b => Code::KeyB,
        0x0c => Code::KeyQ,
        0x0d => Code::KeyW,
        0x0e => Code::KeyE,
        0x0f => Code::KeyR,
        0x10 => Code::KeyY,
        0x11 => Code::KeyT,
        0x12 => Code::Digit1,
        0x13 => Code::Digit2,
        0x14 => Code::Digit3,
        0x15 => Code::Digit4,
        0x16 => Code::Digit6,
        0x17 => Code::Digit5,
        0x18 => Code::Equal,
        0x19 => Code::Digit9,
        0x1a => Code::Digit7,
        0x1b => Code::Minus,
        0x1c => Code::Digit8,
        0x1d => Code::Digit0,
        0x1e => Code::BracketRight,
        0x1f => Code::KeyO,
        0x20 => Code::KeyU,
        0x21 => Code::BracketLeft,
        0x22 => Code::KeyI,
        0x23 => Code::KeyP,
        0x25 => Code::KeyL,
        0x26 => Code::KeyJ,
        0x27 => Code::Quote,
        0x28 => Code::KeyK,
        0x29 => Code::Semicolon,
        0x2a => Code::Backslash,
        0x2b => Code::Comma,
        0x2c => Code::Slash,
        0x2d => Code::KeyN,
        0x2e => Code::KeyM,
        0x2f => Code::Period,
        0x32 => Code::Backquote,
        _ => return None,
    })
}

#[cfg(any(target_os = "macos", test))]
fn resolve_character_key(
    character: u8,
    mut translate_unmodified: impl FnMut(u16) -> Option<Vec<u16>>,
) -> Result<(Code, u16), String> {
    let mut unsupported_key = None;
    for virtual_key in 0..128 {
        let Some(characters) = translate_unmodified(virtual_key) else {
            continue;
        };
        if characters.as_slice() != [u16::from(character)] {
            continue;
        }
        if let Some(code) = code_for_macos_key(virtual_key) {
            return Ok((code, virtual_key));
        }
        // A numeric keypad decimal is not a substitute for the typing key.
        if virtual_key != 0x41 {
            unsupported_key = Some(virtual_key);
        }
    }
    let character = char::from(character);
    match unsupported_key {
        Some(key) => Err(format!(
            "The current keyboard layout places '{character}' on unsupported macOS key 0x{key:02x}. \
             No substitute key was selected"
        )),
        None => Err(format!(
            "The current keyboard layout has no supported typing key that produces '{character}' without modifiers. \
             No substitute key was selected"
        )),
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{Code, resolve_character_key};
    use std::{
        ffi::{c_long, c_ulong, c_void},
        ptr::NonNull,
    };

    const KEY_ACTION_DOWN: u16 = 0;
    const NO_DEAD_KEYS_MASK: u32 = 1;

    // Signatures and constants follow the installed Apple SDK's
    // TextInputSources.h, Events.h, UnicodeUtilities.h, CFBase.h and CFData.h.
    // UniCharCount is unsigned long, not UInt32 (64 bits on supported Macs).
    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        fn TISCopyCurrentKeyboardLayoutInputSource() -> *mut c_void;
        fn TISGetInputSourceProperty(
            input_source: *mut c_void,
            property_key: *const c_void,
        ) -> *mut c_void;
        static kTISPropertyUnicodeKeyLayoutData: *const c_void;
        fn LMGetKbdType() -> u8;
    }

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn UCKeyTranslate(
            key_layout: *const c_void,
            virtual_key: u16,
            key_action: u16,
            modifiers: u32,
            keyboard_type: u32,
            options: u32,
            dead_key_state: *mut u32,
            max_length: c_ulong,
            actual_length: *mut c_ulong,
            characters: *mut u16,
        ) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(value: *const c_void);
        fn CFGetTypeID(value: *const c_void) -> c_ulong;
        fn CFDataGetTypeID() -> c_ulong;
        fn CFDataGetLength(data: *const c_void) -> c_long;
        fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
    }

    struct InputSource(NonNull<c_void>);

    impl Drop for InputSource {
        fn drop(&mut self) {
            // SAFETY: The Copy function gives us one owned, non-null CF object.
            unsafe { CFRelease(self.0.as_ptr()) };
        }
    }

    pub(super) fn character_key(character: u8) -> Result<Code, String> {
        // SAFETY: Called on the UI main thread. No app keyboard state is changed.
        let source = InputSource(
            NonNull::new(unsafe { TISCopyCurrentKeyboardLayoutInputSource() }).ok_or(
                "macOS did not provide the current keyboard layout. Restart MyImage after selecting a keyboard layout",
            )?,
        );
        // SAFETY: This property is documented as borrowed CFData. Retaining the
        // input source for this entire function keeps the property alive.
        let data = unsafe {
            TISGetInputSourceProperty(source.0.as_ptr(), kTISPropertyUnicodeKeyLayoutData)
        };
        if data.is_null() {
            return Err(
                "The current keyboard input source has no Unicode layout data. \
                 Select a keyboard layout and restart MyImage; no substitute key was selected"
                    .to_owned(),
            );
        }
        // SAFETY: A non-null Get-rule CF object may be queried for its type.
        if unsafe { CFGetTypeID(data) != CFDataGetTypeID() || CFDataGetLength(data) <= 0 } {
            return Err(
                "macOS returned invalid keyboard layout data. No substitute key was selected"
                    .to_owned(),
            );
        }
        // SAFETY: We checked that this is nonempty CFData, owned by `source`.
        let layout = unsafe { CFDataGetBytePtr(data) };
        if layout.is_null() {
            return Err(
                "macOS returned an empty keyboard layout. No substitute key was selected"
                    .to_owned(),
            );
        }
        // SAFETY: Main-thread call to the system's current keyboard type query.
        let keyboard_type = u32::from(unsafe { LMGetKbdType() });
        let (code, virtual_key) = resolve_character_key(character, |key| {
            let mut characters = [0_u16; 255];
            let mut length = 0 as c_ulong;
            let mut dead_key_state = 0_u32;
            // SAFETY: `layout` comes from Apple's Unicode layout property and
            // remains alive. All output buffers have the advertised capacities.
            // Each translation starts with fresh dead-key state and no modifiers.
            let status = unsafe {
                UCKeyTranslate(
                    layout.cast(),
                    key,
                    KEY_ACTION_DOWN,
                    0,
                    keyboard_type,
                    NO_DEAD_KEYS_MASK,
                    &mut dead_key_state,
                    characters.len() as c_ulong,
                    &mut length,
                    characters.as_mut_ptr(),
                )
            };
            if status != 0 || dead_key_state != 0 || length as usize > characters.len() {
                return None;
            }
            Some(characters[..length as usize].to_vec())
        })?;
        log::info!(
            "Capture shortcut character '{}' maps to macOS virtual key \
             0x{virtual_key:02x} ({code:?}); restart MyImage after changing layouts",
            char::from(character)
        );
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(entries: &[(u16, &str)]) -> Result<(Code, u16), String> {
        resolve_key(b'.', entries)
    }

    fn resolve_key(character: u8, entries: &[(u16, &str)]) -> Result<(Code, u16), String> {
        resolve_character_key(character, |key| {
            entries
                .iter()
                .find(|entry| entry.0 == key)
                .map(|(_, text)| text.encode_utf16().collect())
        })
    }

    #[test]
    fn capture_chords_include_control_c_and_preserve_platform_alternatives() {
        for include_command in [false, true] {
            let plan = capture_hotkeys_for_keys(Ok(Code::Period), Ok(Code::KeyC), include_command);
            assert!(plan.errors.is_empty());
            let mut expected = vec![
                ("Ctrl+Shift+C", Modifiers::CONTROL, Code::KeyC),
                ("Ctrl+Shift+.", Modifiers::CONTROL, Code::Period),
            ];
            if include_command {
                expected.push(("Cmd+Shift+.", Modifiers::SUPER, Code::Period));
            }
            assert_eq!(plan.hotkeys.len(), expected.len());
            for (actual, (label, modifier, code)) in plan.hotkeys.iter().zip(expected) {
                assert_eq!(actual.label, label);
                assert_eq!(
                    actual.hotkey.id(),
                    HotKey::new(Some(modifier | Modifiers::SHIFT), code).id()
                );
            }
        }
    }

    #[test]
    fn letter_c_uses_its_layout_key_and_rejects_missing_or_composed_characters() {
        assert_eq!(
            resolve_key(b'c', &[(0x08, "c")]).unwrap(),
            (Code::KeyC, 0x08)
        );
        let relocated = resolve_key(b'c', &[(0x08, "j"), (0x22, "c")]).unwrap();
        assert_eq!(relocated, (Code::KeyI, 0x22));
        let plan = capture_hotkeys_for_keys(Ok(Code::Slash), Ok(relocated.0), true);
        assert_eq!(
            plan.hotkeys[0].hotkey.id(),
            HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyI).id()
        );
        assert_eq!(
            plan.hotkeys[1].hotkey.id(),
            HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Slash).id()
        );
        assert!(resolve_key(b'c', &[]).is_err());
        assert!(resolve_key(b'c', &[(0x08, "ç")]).is_err());
        assert!(resolve_key(b'c', &[(0x08, "cx")]).is_err());
    }

    #[test]
    fn unavailable_layout_character_keeps_other_capture_chords_available() {
        let plan = capture_hotkeys_for_keys(Err("missing period".into()), Ok(Code::KeyC), true);
        assert_eq!(plan.hotkeys.len(), 1);
        assert_eq!(plan.hotkeys[0].label, "Ctrl+Shift+C");
        assert!(plan.errors[0].contains("missing period"));

        let plan = capture_hotkeys_for_keys(Ok(Code::Period), Err("missing c".into()), true);
        assert_eq!(plan.hotkeys.len(), 2);
        assert_eq!(plan.hotkeys[0].label, "Ctrl+Shift+.");
        assert_eq!(plan.hotkeys[1].label, "Cmd+Shift+.");
        assert!(plan.errors[0].contains("missing c"));

        let plan =
            capture_hotkeys_for_keys(Err("missing period".into()), Err("missing c".into()), true);
        assert!(plan.hotkeys.is_empty());
        assert_eq!(plan.errors.len(), 2);
    }

    #[test]
    fn us_period_keeps_its_key_and_ignores_keypad_decimal() {
        assert_eq!(
            resolve(&[(0x2f, "."), (0x41, ".")]).unwrap(),
            (Code::Period, 0x2f)
        );
    }

    #[test]
    fn relocated_period_uses_the_current_layout_not_the_us_position() {
        assert_eq!(
            resolve(&[(0x2f, "ç"), (0x2c, ".")]).unwrap(),
            (Code::Slash, 0x2c)
        );
        assert_eq!(resolve(&[(0x2e, ".")]).unwrap(), (Code::KeyM, 0x2e));
    }

    #[test]
    fn absent_or_composed_period_never_registers_a_fallback_chord() {
        assert!(resolve(&[]).is_err());
        assert!(resolve(&[(0x2f, ">"), (0x41, ".")]).is_err());
        assert!(resolve(&[(0x2f, ".x")]).is_err());
        assert!(resolve(&[(0x2f, "…")]).is_err());
    }

    #[test]
    fn unsupported_iso_key_is_reported_instead_of_mapping_to_another_key() {
        let error = resolve(&[(0x0a, ".")]).unwrap_err();
        assert!(error.contains("unsupported macOS key 0x0a"));
        assert_eq!(code_for_macos_key(0x0a), None);
        assert_eq!(code_for_macos_key(0x41), None);
    }
}
