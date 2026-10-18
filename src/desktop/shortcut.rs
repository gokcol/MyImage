//! Resolve the capture shortcut once during desktop initialization.
//!
//! macOS registers physical virtual keys, so the US `Code::Period` position is
//! not necessarily the key labeled '.' on the active layout. Resolve the
//! unmodified character first, then register Control + Shift and Command + Shift
//! with that same key.
//! Restart MyImage after changing keyboard layouts. Windows continues to use
//! global-hotkey's `Code::Period` mapping to `VK_OEM_PERIOD`.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};

pub(super) struct CaptureHotkey {
    pub label: &'static str,
    pub hotkey: HotKey,
}

/// Call on the main UI thread; Apple's keyboard-layout APIs are not thread safe.
/// If layout resolution fails, the caller exposes the error and keeps Capture
/// available in the window/tray instead of registering an unrelated fallback.
pub(super) fn capture_hotkeys() -> Result<Vec<CaptureHotkey>, String> {
    #[cfg(target_os = "macos")]
    let key = macos::period_key()?;
    #[cfg(not(target_os = "macos"))]
    let key = Code::Period;

    // CONTROL deliberately means Control on macOS, not Command.
    let control = CaptureHotkey {
        label: "Ctrl+Shift+.",
        hotkey: HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), key),
    };
    #[cfg(target_os = "macos")]
    {
        Ok(vec![
            control,
            CaptureHotkey {
                label: "Cmd+Shift+.",
                hotkey: HotKey::new(Some(Modifiers::SUPER | Modifiers::SHIFT), key),
            },
        ])
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(vec![control])
    }
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
fn resolve_period_key(
    mut translate_unmodified: impl FnMut(u16) -> Option<Vec<u16>>,
) -> Result<(Code, u16), String> {
    let mut unsupported_period = None;
    for virtual_key in 0..128 {
        let Some(characters) = translate_unmodified(virtual_key) else {
            continue;
        };
        if characters.as_slice() != [u16::from(b'.')] {
            continue;
        }
        if let Some(code) = code_for_macos_key(virtual_key) {
            return Ok((code, virtual_key));
        }
        // A numeric keypad decimal is not a substitute for the typing key.
        if virtual_key != 0x41 {
            unsupported_period = Some(virtual_key);
        }
    }
    match unsupported_period {
        Some(key) => Err(format!(
            "The current keyboard layout places '.' on unsupported macOS key 0x{key:02x}. \
             No fallback shortcut was registered"
        )),
        None => Err(
            "The current keyboard layout has no supported typing key that produces '.' without modifiers. \
             No fallback shortcut was registered"
                .to_owned(),
        ),
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{Code, resolve_period_key};
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

    pub(super) fn period_key() -> Result<Code, String> {
        // SAFETY: Called on the UI main thread. No app keyboard state is changed.
        let source = InputSource(
            NonNull::new(unsafe { TISCopyCurrentKeyboardLayoutInputSource() }).ok_or(
                "macOS did not provide the current keyboard layout. No fallback shortcut was registered",
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
                 Select a keyboard layout and restart MyImage; no fallback shortcut was registered"
                    .to_owned(),
            );
        }
        // SAFETY: A non-null Get-rule CF object may be queried for its type.
        if unsafe { CFGetTypeID(data) != CFDataGetTypeID() || CFDataGetLength(data) <= 0 } {
            return Err(
                "macOS returned invalid keyboard layout data. No fallback shortcut was registered"
                    .to_owned(),
            );
        }
        // SAFETY: We checked that this is nonempty CFData, owned by `source`.
        let layout = unsafe { CFDataGetBytePtr(data) };
        if layout.is_null() {
            return Err(
                "macOS returned an empty keyboard layout. No fallback shortcut was registered"
                    .to_owned(),
            );
        }
        // SAFETY: Main-thread call to the system's current keyboard type query.
        let keyboard_type = u32::from(unsafe { LMGetKbdType() });
        let (code, virtual_key) = resolve_period_key(|key| {
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
            "Capture shortcuts: Control+Shift+'.' and Command+Shift+'.' map to macOS virtual key \
             0x{virtual_key:02x} ({code:?}); restart MyImage after changing layouts"
        );
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(entries: &[(u16, &str)]) -> Result<(Code, u16), String> {
        resolve_period_key(|key| {
            entries
                .iter()
                .find(|entry| entry.0 == key)
                .map(|(_, text)| text.encode_utf16().collect())
        })
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
