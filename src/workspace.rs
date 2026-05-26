//! Workspace observation: idle/typing detection, frontmost app, fullscreen, text caret.
//!
//! macOS-specific polling is added in later commits; this file currently defines
//! only the platform-independent snapshot/tick types and the is_busy/is_idle policy.

use crate::physics::{Rect, Vec2};

#[derive(Clone, Debug)]
pub struct WorkspaceSnapshot {
    /// false on non-macOS stub builds or before the first successful poll on macOS.
    /// Gates is_busy()/is_idle() so the pet falls through to Idle on stub builds.
    pub workspace_available: bool,
    pub seconds_idle: f32,
    pub typing_rate_per_sec: f32,
    pub frontmost_bundle_id: Option<String>,
    pub frontmost_is_editor: bool,
    /// Pet-space points, top-left origin (see spec §Coordinate system).
    pub caret_rect: Option<Rect>,
    /// On the pet's active display only.
    pub fullscreen_active: bool,
    /// Pet-space points, top-left origin.
    pub cursor_pos: Vec2,
}

impl Default for WorkspaceSnapshot {
    fn default() -> Self {
        Self {
            workspace_available: false,
            seconds_idle: 0.0,
            typing_rate_per_sec: 0.0,
            frontmost_bundle_id: None,
            frontmost_is_editor: false,
            caret_rect: None,
            fullscreen_active: false,
            cursor_pos: Vec2 { x: 0.0, y: 0.0 },
        }
    }
}

/// Keys-per-second rate above which the user is considered to be typing.
const TYPING_BUSY_THRESHOLD: f32 = 1.0;
/// Seconds of inactivity below which the user is still considered recently active (busy).
const BUSY_IDLE_SECS: f32 = 2.0;
/// Seconds of inactivity at or above which the user is considered idle.
const IDLE_SECS: f32 = 5.0;

impl WorkspaceSnapshot {
    pub fn is_busy(&self) -> bool {
        self.workspace_available
            && (self.frontmost_is_editor
                || self.typing_rate_per_sec > TYPING_BUSY_THRESHOLD
                || self.seconds_idle < BUSY_IDLE_SECS)
    }

    pub fn is_idle(&self) -> bool {
        self.workspace_available && self.seconds_idle >= IDLE_SECS && !self.is_busy()
    }
}

/// Owned result of one observer tick. Returning an owned value (not `&WorkspaceSnapshot`)
/// releases the `&mut WorkspaceObserver` borrow immediately, so the app can then call
/// `is_accessibility_trusted()` or `sync_settings_window()` without fighting the borrow
/// checker.
#[derive(Clone, Debug)]
pub struct WorkspaceTick {
    pub snapshot: WorkspaceSnapshot,
    /// True if `is_accessibility_trusted()` flipped during this tick vs the previous one.
    pub trust_changed: bool,
}

/// Convert a y coordinate from Cocoa global space (origin = primary display bottom-left, Y up)
/// to Quartz space (origin = primary display top-left, Y down). x is unchanged across the
/// two spaces. The pivot is the primary display's logical height, so this is correct for
/// points on any secondary display, including displays with negative coordinates or
/// vertical layouts — both spaces share the primary display as their anchor.
pub fn cocoa_to_quartz_y(cocoa_y: f32, primary_display_height: f32) -> f32 {
    primary_display_height - cocoa_y
}

use std::time::Instant;

pub struct WorkspaceObserver {
    last_known_ax_trusted: Option<bool>,
    pub(crate) prompted_for_accessibility_at_startup: bool,
    active_display: Option<DisplayInfo>,
    last_snapshot: WorkspaceSnapshot,
    last_tick_at: Option<Instant>,
    last_key_counter: Option<i64>,
    last_frontmost_poll_at: Option<Instant>,
    last_fullscreen_poll_at: Option<Instant>,
    last_caret_poll_at: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayInfo {
    /// monitor.name(); diagnostic only, not unique
    pub name: Option<String>,
    /// pet-space, top-left origin, points
    pub bounds_logical: Rect,
    /// window.scale_factor()
    pub scale_factor: f32,
    /// height in points of the primary display, used as Y-flip pivot
    pub primary_display_height: f32,
}

impl WorkspaceObserver {
    pub fn new() -> Self {
        Self {
            last_known_ax_trusted: None,
            prompted_for_accessibility_at_startup: false,
            active_display: None,
            last_snapshot: WorkspaceSnapshot::default(),
            last_tick_at: None,
            last_key_counter: None,
            last_frontmost_poll_at: None,
            last_fullscreen_poll_at: None,
            last_caret_poll_at: None,
        }
    }

    pub fn set_active_display(&mut self, info: Option<DisplayInfo>) {
        self.active_display = info;
    }

    pub fn tick(&mut self, now: Instant) -> WorkspaceTick {
        let seconds_idle = macos_polling::seconds_since_last_input();
        let key_counter = macos_polling::key_down_counter();

        let typing_rate_per_sec = match (self.last_tick_at, self.last_key_counter) {
            (Some(prev_at), Some(prev_counter)) => {
                let elapsed = now.saturating_duration_since(prev_at).as_secs_f32();
                if elapsed > 0.0 {
                    ((key_counter - prev_counter).max(0) as f32) / elapsed
                } else {
                    0.0
                }
            }
            _ => 0.0,
        };

        self.last_tick_at = Some(now);
        self.last_key_counter = Some(key_counter);

        let (frontmost_bundle_id, frontmost_is_editor) = {
            let due = self
                .last_frontmost_poll_at
                .is_none_or(|t| now.saturating_duration_since(t) >= std::time::Duration::from_millis(500));
            if due {
                let id = macos_polling::frontmost_bundle_id();
                let is_editor = id.as_deref().is_some_and(is_editor_bundle_id);
                self.last_frontmost_poll_at = Some(now);
                (id, is_editor)
            } else {
                (
                    self.last_snapshot.frontmost_bundle_id.clone(),
                    self.last_snapshot.frontmost_is_editor,
                )
            }
        };

        let fullscreen_active = {
            let due = self
                .last_fullscreen_poll_at
                .is_none_or(|t| now.saturating_duration_since(t) >= std::time::Duration::from_millis(500));
            match (due, self.active_display.as_ref()) {
                (true, Some(display)) => {
                    self.last_fullscreen_poll_at = Some(now);
                    let pid = std::process::id() as i32;
                    macos_polling::any_fullscreen_on(display.bounds_logical, pid)
                }
                _ => self.last_snapshot.fullscreen_active,
            }
        };

        let caret_rect = {
            let due = self
                .last_caret_poll_at
                .is_none_or(|t| now.saturating_duration_since(t) >= std::time::Duration::from_millis(250));
            if due && self.is_accessibility_trusted() {
                self.last_caret_poll_at = Some(now);
                macos_polling::caret_rect_quartz()
            } else if due {
                self.last_caret_poll_at = Some(now);
                None
            } else {
                self.last_snapshot.caret_rect
            }
        };

        let cursor_pos = if let Some(display) = self.active_display.as_ref() {
            let (cx, cy_cocoa) = macos_polling::cursor_cocoa_location();
            let cy_quartz = cocoa_to_quartz_y(cy_cocoa, display.primary_display_height);
            crate::physics::Vec2 { x: cx, y: cy_quartz }
        } else {
            self.last_snapshot.cursor_pos
        };

        self.last_snapshot = WorkspaceSnapshot {
            workspace_available: cfg!(target_os = "macos"),
            seconds_idle,
            typing_rate_per_sec,
            frontmost_bundle_id,
            frontmost_is_editor,
            fullscreen_active,
            caret_rect,
            cursor_pos,
        };

        let now_trusted = self.is_accessibility_trusted();
        let trust_changed = match self.last_known_ax_trusted {
            Some(prev) => prev != now_trusted,
            None => false,
        };
        self.last_known_ax_trusted = Some(now_trusted);

        WorkspaceTick {
            snapshot: self.last_snapshot.clone(),
            trust_changed,
        }
    }

    pub fn is_accessibility_trusted(&self) -> bool {
        macos_polling::is_ax_trusted()
    }

    pub fn request_accessibility_on_startup_if_enabled(&mut self, avoid_text_cursor: bool) {
        if !avoid_text_cursor || self.prompted_for_accessibility_at_startup {
            return;
        }
        self.prompted_for_accessibility_at_startup = true;
        let _ = macos_polling::ax_request_prompt();
    }

    pub fn request_accessibility_now(&mut self) {
        let _ = macos_polling::ax_request_prompt();
    }
}

impl Default for WorkspaceObserver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
mod macos_polling {
    use objc2_app_kit::{NSEvent, NSWorkspace};
    use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGEventType};

    /// Current mouse pointer location in Cocoa global coordinates
    /// (origin = primary display bottom-left, Y up). Caller is responsible for
    /// Y-flipping to Quartz space via `cocoa_to_quartz_y` if needed.
    ///
    /// `NSEvent::mouseLocation` is a class method that reads the live cursor
    /// position regardless of whether any event has been dispatched, so it's
    /// safe to poll every tick. In objc2-app-kit 0.3 the wrapper is exposed as
    /// a safe `pub fn` (no `unsafe` block required).
    pub fn cursor_cocoa_location() -> (f32, f32) {
        let p = NSEvent::mouseLocation();
        (p.x as f32, p.y as f32)
    }

    /// kCGAnyInputEventType in C is `~0u32` (0xFFFFFFFF). The objc2-core-graphics
    /// crate doesn't expose a named constant for it (the same raw value is reused
    /// in `CGEventType::TapDisabledByUserInput`), so we construct it explicitly.
    /// Passing this sentinel to `seconds_since_last_event_type` returns the
    /// seconds since the most recent event of ANY input type (mouse + keyboard).
    const ANY_INPUT_EVENT: CGEventType = CGEventType(!0u32);

    /// Seconds since the most recent input event (mouse or keyboard) globally.
    pub fn seconds_since_last_input() -> f32 {
        let secs = CGEventSource::seconds_since_last_event_type(
            CGEventSourceStateID::CombinedSessionState,
            ANY_INPUT_EVENT,
        );
        if secs.is_finite() && secs > 0.0 { secs as f32 } else { 0.0 }
    }

    /// Cumulative count of key-down events since the session started.
    /// `counter_for_event_type` returns u32; we widen to i64 so a u32
    /// wraparound is still representable when subtracting two samples.
    pub fn key_down_counter() -> i64 {
        let count = CGEventSource::counter_for_event_type(
            CGEventSourceStateID::CombinedSessionState,
            CGEventType::KeyDown, // matches kCGEventKeyDown = 10
        );
        count as i64
    }

    /// Bundle identifier of the frontmost (key-focused) application, or `None`
    /// if no app is frontmost or the app has no bundle identifier (rare). The
    /// underlying `NSWorkspace` call is safe and cheap (~hundreds of ns).
    pub fn frontmost_bundle_id() -> Option<String> {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        app.bundleIdentifier().map(|s| s.to_string())
    }

    use crate::physics::Rect as PetRect;
    use objc2_core_foundation::{CFDictionary, CFNumber, CFString, CGRect};
    use objc2_core_graphics::{
        kCGNullWindowID, CGRectMakeWithDictionaryRepresentation, CGWindowListCopyWindowInfo,
        CGWindowListOption,
    };
    use std::ffi::c_void;

    /// Returns true if any on-screen, non-overlay, non-own-process window covers
    /// `active_bounds` within 1 pt on each side. The spec's filter rules
    /// (layer == 0, owner_pid != our_pid) match what AppKit considers a normal
    /// application window — the layered windows (menu bar, dock overlays,
    /// Spotlight, screensaver, etc.) live on non-zero layers and are skipped.
    ///
    /// Implementation notes / deviations from the plan draft:
    /// - `CGWindowListCopyWindowInfo` in objc2-core-graphics 0.3 returns
    ///   `Option<CFRetained<CFArray>>` (an untyped array). Element pointers are
    ///   raw `*const c_void` that we cast to `*const CFDictionary` and
    ///   dereference under unsafe.
    /// - The dict lookup uses the typed wrapper `CFDictionary::value_if_present`
    ///   from objc2-core-foundation 0.3, passing CFString keys as `*const c_void`.
    /// - `CFNumber::as_i64` does the unwrapping the plan draft did manually via
    ///   `CFNumberGetValue` + `SInt64Type`.
    /// - `kCGNullWindowID` is a `pub const` in objc2-core-graphics 0.3 (not a
    ///   newtype wrapper like the plan draft assumed).
    ///
    /// **macOS 14+ caveat:** `CGWindowListCopyWindowInfo` returns reduced bounds info for
    /// other-application windows when the calling app lacks Screen Recording permission.
    /// On those systems, this function may silently fail to detect cross-app fullscreen
    /// (own-app windows are unaffected). The pet does not request that permission, so
    /// in practice we accept this limitation as the "safe default" of leaving the pet
    /// visible when we can't confirm fullscreen.
    pub fn any_fullscreen_on(active_bounds: PetRect, our_pid: i32) -> bool {
        let Some(info) = CGWindowListCopyWindowInfo(
            CGWindowListOption::OptionOnScreenOnly,
            kCGNullWindowID,
        ) else {
            return false;
        };

        let key_layer = CFString::from_static_str("kCGWindowLayer");
        let key_owner_pid = CFString::from_static_str("kCGWindowOwnerPID");
        let key_bounds = CFString::from_static_str("kCGWindowBounds");

        let count = info.count();
        for i in 0..count {
            // SAFETY: `i` is in `0..count`, so the index is in bounds.
            // The array isn't mutated during this borrow.
            let dict_ptr = unsafe { info.value_at_index(i) } as *const CFDictionary;
            if dict_ptr.is_null() {
                continue;
            }
            // SAFETY: CGWindowListCopyWindowInfo guarantees each element is a
            // valid (non-NULL) CFDictionary owned by the CFArray we hold.
            let dict = unsafe { &*dict_ptr };

            // Filter: kCGWindowLayer != 0 → skip overlay/menubar/etc.
            let layer = match dict_get_i64(dict, &key_layer) {
                Some(v) => v,
                None => continue,
            };
            if layer != 0 {
                continue;
            }
            // Filter: own process → skip (our pet's own panel must not register
            // as a fullscreen app).
            let owner_pid = dict_get_i64(dict, &key_owner_pid).unwrap_or(0) as i32;
            if owner_pid == our_pid {
                continue;
            }

            // Read kCGWindowBounds → CFDictionary → CGRect.
            let Some(bounds_dict) = dict_get_dict(dict, &key_bounds) else {
                continue;
            };
            // SAFETY: bounds_dict points at a valid CFDictionary owned by the
            // outer dict for as long as the outer dict is alive (which is the
            // whole loop). We pass it as &CFDictionary to the FFI call.
            let bounds_ref: &CFDictionary = unsafe { &*bounds_dict };
            let mut rect = CGRect::default();
            // SAFETY: `bounds_ref` is a valid CFDictionary, `rect` is a valid
            // out-param pointer.
            let ok = unsafe {
                CGRectMakeWithDictionaryRepresentation(Some(bounds_ref), &mut rect as *mut CGRect)
            };
            if !ok {
                continue;
            }

            let win = PetRect {
                min: crate::physics::Vec2 {
                    x: rect.origin.x as f32,
                    y: rect.origin.y as f32,
                },
                max: crate::physics::Vec2 {
                    x: (rect.origin.x + rect.size.width) as f32,
                    y: (rect.origin.y + rect.size.height) as f32,
                },
            };
            if rects_equal_within(&win, &active_bounds, 1.0) {
                return true;
            }
        }
        false
    }

    fn rects_equal_within(a: &PetRect, b: &PetRect, tol: f32) -> bool {
        (a.min.x - b.min.x).abs() <= tol
            && (a.min.y - b.min.y).abs() <= tol
            && (a.max.x - b.max.x).abs() <= tol
            && (a.max.y - b.max.y).abs() <= tol
    }

    /// Look up a key in a CFDictionary and decode it as i64 via CFNumber.
    fn dict_get_i64(dict: &CFDictionary, key: &CFString) -> Option<i64> {
        let key_ptr: *const c_void = (key as *const CFString).cast();
        let mut value: *const c_void = std::ptr::null();
        // SAFETY: `dict` is a valid CFDictionary, `key_ptr` is a valid CFString
        // pointer (a CFType), `&mut value` is a valid out-param pointer.
        let found = unsafe { dict.value_if_present(key_ptr, &mut value) };
        if !found || value.is_null() {
            return None;
        }
        // SAFETY: kCGWindowLayer / kCGWindowOwnerPID values are documented as
        // CFNumber; the dict was produced by CGWindowListCopyWindowInfo.
        let number: &CFNumber = unsafe { &*(value as *const CFNumber) };
        number.as_i64()
    }

    /// Look up a key in a CFDictionary and return the value pointer cast to
    /// `*const CFDictionary` if present (caller must verify the value really
    /// is a dictionary; we only use this for `kCGWindowBounds` which is
    /// documented as a CFDictionary).
    fn dict_get_dict(dict: &CFDictionary, key: &CFString) -> Option<*const CFDictionary> {
        let key_ptr: *const c_void = (key as *const CFString).cast();
        let mut value: *const c_void = std::ptr::null();
        // SAFETY: see dict_get_i64.
        let found = unsafe { dict.value_if_present(key_ptr, &mut value) };
        if !found || value.is_null() {
            None
        } else {
            Some(value as *const CFDictionary)
        }
    }

    use objc2_application_services::{
        kAXTrustedCheckOptionPrompt, kAXValueCGRectType, AXIsProcessTrusted,
        AXIsProcessTrustedWithOptions, AXUIElement, AXValue,
    };
    use objc2_core_foundation::{kCFBooleanTrue, CFMutableDictionary, CFType};

    /// Returns true if this process is trusted for the Accessibility API (System
    /// Settings → Privacy & Security → Accessibility). The call is cheap and safe
    /// to invoke every tick.
    pub fn is_ax_trusted() -> bool {
        // SAFETY: AXIsProcessTrusted has no inputs and no side effects beyond
        // returning the bool. The free-function form is marked `unsafe` only
        // because all extern-C wrappers in the crate are.
        unsafe { AXIsProcessTrusted() }
    }

    /// Calls `AXIsProcessTrustedWithOptions({kAXTrustedCheckOptionPrompt: true})`
    /// to show the system Accessibility prompt asynchronously when the process is
    /// not trusted. The return value mirrors `AXIsProcessTrusted()` and does not
    /// reflect the user's eventual response; we discard it at the call site.
    ///
    /// Implementation notes / deviations from the plan draft:
    /// - `objc2-core-foundation 0.3.2` does not expose
    ///   `CFDictionary::from_pairs` or a `CFBoolean::true_value()` helper. We
    ///   build the options dict via `CFMutableDictionary::new` with the standard
    ///   type-aware key/value callbacks, then `add_value` with the global
    ///   `kCFBooleanTrue` static.
    /// - `kAXTrustedCheckOptionPrompt` *is* exported as a `pub static
    ///   &'static CFString` under the `HIServices` feature, so we use the named
    ///   constant rather than the literal-string fallback.
    pub fn ax_request_prompt() -> bool {
        use objc2_core_foundation::{
            kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks,
        };
        use std::ffi::c_void;

        // SAFETY: passing standard CFType callbacks for keys and values
        // matches the dictionary contents we build below (CFString key,
        // CFBoolean value). NULL allocator => default CFAllocator.
        let dict = unsafe {
            CFMutableDictionary::new(
                None,
                1,
                &raw const kCFTypeDictionaryKeyCallBacks,
                &raw const kCFTypeDictionaryValueCallBacks,
            )
        };
        let Some(dict) = dict else {
            // Allocation failure is exceedingly rare; treat the same as the
            // trust check returning false (which is what the OS would say
            // without options anyway).
            return false;
        };

        // SAFETY: kAXTrustedCheckOptionPrompt and kCFBooleanTrue are
        // process-lifetime statics owned by the system frameworks. Both
        // pointers stay valid for the entire AXIsProcessTrustedWithOptions
        // call. CFType callbacks will retain them.
        unsafe {
            let key_ptr: *const c_void = (kAXTrustedCheckOptionPrompt
                as *const objc2_core_foundation::CFString)
                .cast();
            // kCFBooleanTrue is Option<&'static CFBoolean>; on macOS it is
            // always Some, but we guard for completeness.
            let Some(value) = kCFBooleanTrue else {
                return false;
            };
            let value_ptr: *const c_void =
                (value as *const objc2_core_foundation::CFBoolean).cast();
            CFMutableDictionary::add_value(Some(&dict), key_ptr, value_ptr);
        }

        // SAFETY: `dict` is a valid immutable view of our newly-built
        // CFMutableDictionary; AX only reads from it.
        unsafe { AXIsProcessTrustedWithOptions(Some(dict.as_opaque())) }
    }

    /// Best-effort caret bounding rect from the system-wide focused UI element,
    /// expressed in Quartz global screen points (origin = primary display
    /// top-left, Y down). Returns `None` whenever any step of the AX chain
    /// fails: no focused element, focus doesn't expose
    /// `AXSelectedTextRange`/`AXBoundsForRange`, empty selection, or the target
    /// app stalls past the 100 ms messaging timeout.
    ///
    /// Implementation notes / deviations from spec:
    /// - `objc2-application-services 0.3.2` does not export
    ///   `kAXFocusedUIElementAttribute` / `kAXSelectedTextRangeAttribute` /
    ///   `kAXBoundsForRangeParameterizedAttribute` as constants, so we build
    ///   them as static CFStrings.
    /// - The wrappers `AXUIElement::copy_attribute_value` /
    ///   `copy_parameterized_attribute_value` take an out-pointer
    ///   (`NonNull<*const CFType>`) and return `AXError`; the spec's
    ///   `.ok()?` pattern doesn't apply. We check `AXError::Success` and use
    ///   `CFRetained::from_raw` to take ownership of the returned CF object.
    /// - `CFRetained::downcast` is safe and returns `Result`; we use it to go
    ///   from CFType → AXUIElement / AXValue.
    pub fn caret_rect_quartz() -> Option<PetRect> {
        use objc2_application_services::AXError;
        use objc2_core_foundation::{CFRetained, CGRect};
        use std::ptr::NonNull;

        // SAFETY: AXUIElementCreateSystemWide returns a valid retained AXUIElement.
        let systemwide: CFRetained<AXUIElement> = unsafe { AXUIElement::new_system_wide() };
        // SAFETY: timeout value 0.1 is a valid positive float; element is valid.
        let _ = unsafe { systemwide.set_messaging_timeout(0.1) };

        // Step 1: focused UI element from systemwide.
        let focused_attr = CFString::from_static_str("AXFocusedUIElement");
        let mut focused_raw: *const CFType = std::ptr::null();
        // SAFETY: out-pointer is valid for the duration of the call. The
        // function returns AXError; on Success it writes a retained CFType ref
        // to the out-pointer (or null on rare misbehavior).
        let err = unsafe {
            systemwide.copy_attribute_value(
                &focused_attr,
                NonNull::new_unchecked(&mut focused_raw as *mut *const CFType),
            )
        };
        if err != AXError::Success || focused_raw.is_null() {
            return None;
        }
        // SAFETY: AX gave us ownership of a retained CFType non-null pointer.
        let focused_cf: CFRetained<CFType> =
            unsafe { CFRetained::from_raw(NonNull::new_unchecked(focused_raw as *mut CFType)) };
        let focused: CFRetained<AXUIElement> = focused_cf.downcast::<AXUIElement>().ok()?;
        // SAFETY: timeout value is valid; element is valid.
        let _ = unsafe { focused.set_messaging_timeout(0.1) };

        // Step 2: selected text range from focused element.
        let range_attr = CFString::from_static_str("AXSelectedTextRange");
        let mut range_raw: *const CFType = std::ptr::null();
        // SAFETY: see above.
        let err = unsafe {
            focused.copy_attribute_value(
                &range_attr,
                NonNull::new_unchecked(&mut range_raw as *mut *const CFType),
            )
        };
        if err != AXError::Success || range_raw.is_null() {
            return None;
        }
        // SAFETY: AX gave us ownership of a retained CFType non-null pointer.
        let range_value: CFRetained<CFType> =
            unsafe { CFRetained::from_raw(NonNull::new_unchecked(range_raw as *mut CFType)) };

        // Step 3: parameterized bounds-for-range on focused element.
        let bounds_attr = CFString::from_static_str("AXBoundsForRange");
        let mut bounds_raw: *const CFType = std::ptr::null();
        // SAFETY: out-pointer valid; `range_value` is a valid CFType the
        // focused element knows how to interpret as an AXValue<CFRange>.
        let err = unsafe {
            focused.copy_parameterized_attribute_value(
                &bounds_attr,
                &range_value,
                NonNull::new_unchecked(&mut bounds_raw as *mut *const CFType),
            )
        };
        if err != AXError::Success || bounds_raw.is_null() {
            return None;
        }
        // SAFETY: see above.
        let bounds_cf: CFRetained<CFType> =
            unsafe { CFRetained::from_raw(NonNull::new_unchecked(bounds_raw as *mut CFType)) };
        let ax_value: CFRetained<AXValue> = bounds_cf.downcast::<AXValue>().ok()?;

        // Step 4: unbox the AXValue into a CGRect.
        let mut rect = CGRect::default();
        // SAFETY: AXValueGetValue writes into `rect` if the type matches; we
        // pass the matching kAXValueCGRectType discriminant. The wrapper
        // signature wants AXValueType, constructed from the discriminator u32.
        let ok = unsafe {
            ax_value.value(
                objc2_application_services::AXValueType(kAXValueCGRectType),
                NonNull::new_unchecked(&mut rect as *mut CGRect as *mut std::ffi::c_void),
            )
        };
        if !ok {
            return None;
        }

        Some(PetRect {
            min: crate::physics::Vec2 {
                x: rect.origin.x as f32,
                y: rect.origin.y as f32,
            },
            max: crate::physics::Vec2 {
                x: (rect.origin.x + rect.size.width) as f32,
                y: (rect.origin.y + rect.size.height) as f32,
            },
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod macos_polling {
    use crate::physics::Rect as PetRect;
    pub fn seconds_since_last_input() -> f32 { 0.0 }
    pub fn key_down_counter() -> i64 { 0 }
    pub fn frontmost_bundle_id() -> Option<String> { None }
    pub fn any_fullscreen_on(_active_bounds: PetRect, _our_pid: i32) -> bool { false }
    pub fn is_ax_trusted() -> bool { true }
    pub fn caret_rect_quartz() -> Option<PetRect> { None }
    pub fn ax_request_prompt() -> bool { true }
    pub fn cursor_cocoa_location() -> (f32, f32) { (0.0, 0.0) }
}

/// Bundle identifiers we consider "editors" for the purpose of marking the user busy.
/// Prefix match supported via the trailing `*` convention; matcher handles it.
const EDITOR_BUNDLE_IDS: &[&str] = &[
    "com.apple.dt.Xcode",
    "com.microsoft.VSCode",
    "com.todesktop.230313mzl4w4u92", // Cursor
    "com.sublimetext.4",
    "com.googlecode.iterm2",
    "com.apple.Terminal",
    "com.mitchellh.ghostty",
    "com.jetbrains.*",
];

pub fn is_editor_bundle_id(bundle_id: &str) -> bool {
    EDITOR_BUNDLE_IDS.iter().any(|pattern| {
        if let Some(prefix) = pattern.strip_suffix('*') {
            bundle_id.starts_with(prefix)
        } else {
            *pattern == bundle_id
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_with(
        editor: bool,
        typing_rate: f32,
        idle: f32,
    ) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            workspace_available: true,
            seconds_idle: idle,
            typing_rate_per_sec: typing_rate,
            frontmost_bundle_id: None,
            frontmost_is_editor: editor,
            caret_rect: None,
            fullscreen_active: false,
            cursor_pos: Vec2 { x: 0.0, y: 0.0 },
        }
    }

    #[test]
    fn busy_when_editor_frontmost() {
        assert!(snapshot_with(true, 0.0, 10.0).is_busy());
    }

    #[test]
    fn busy_when_typing_fast() {
        assert!(snapshot_with(false, 2.0, 10.0).is_busy());
    }

    #[test]
    fn busy_when_recently_active() {
        assert!(snapshot_with(false, 0.0, 1.0).is_busy());
    }

    #[test]
    fn idle_when_long_quiet_and_not_editor() {
        let s = snapshot_with(false, 0.0, 6.0);
        assert!(!s.is_busy());
        assert!(s.is_idle());
    }

    #[test]
    fn between_2_and_5_seconds_is_neither_busy_nor_idle() {
        let s = snapshot_with(false, 0.0, 3.5);
        assert!(!s.is_busy());
        assert!(!s.is_idle());
    }

    #[test]
    fn boundary_at_2s_idle_is_neither_busy_nor_idle() {
        let s = snapshot_with(false, 0.0, 2.0);
        assert!(!s.is_busy(), "seconds_idle == 2.0 should not be busy (condition is < 2.0)");
        assert!(!s.is_idle(), "seconds_idle == 2.0 should not be idle (condition is >= 5.0)");
    }

    #[test]
    fn boundary_at_5s_idle_is_idle() {
        let s = snapshot_with(false, 0.0, 5.0);
        assert!(!s.is_busy());
        assert!(s.is_idle(), "seconds_idle == 5.0 should be idle (condition is >= 5.0)");
    }

    #[test]
    fn is_busy_and_is_idle_never_both_true() {
        for editor in [false, true] {
            for &typing in &[0.0_f32, 0.5, 2.0] {
                for &idle in &[0.0_f32, 1.5, 3.0, 6.0] {
                    let s = snapshot_with(editor, typing, idle);
                    assert!(
                        !(s.is_busy() && s.is_idle()),
                        "both busy and idle for editor={editor} typing={typing} idle={idle}"
                    );
                }
            }
        }
    }

    #[test]
    fn workspace_unavailable_blocks_both_busy_and_idle() {
        let mut s = snapshot_with(true, 10.0, 10.0);
        s.workspace_available = false;
        assert!(!s.is_busy());
        assert!(!s.is_idle());
    }

    #[test]
    fn default_snapshot_is_inert() {
        let s = WorkspaceSnapshot::default();
        assert!(!s.workspace_available);
        assert!(!s.is_busy());
        assert!(!s.is_idle());
        assert!(!s.fullscreen_active);
        assert!(s.caret_rect.is_none());
    }

    #[test]
    fn cocoa_to_quartz_y_on_primary_display() {
        // primary display 900 pt tall, point at cocoa y=800 → quartz y=100.
        assert_eq!(cocoa_to_quartz_y(800.0, 900.0), 100.0);
    }

    #[test]
    fn cocoa_to_quartz_y_on_display_above_primary() {
        // Cocoa y > primary_height means above primary; quartz y is negative.
        assert_eq!(cocoa_to_quartz_y(1400.0, 900.0), -500.0);
    }

    #[test]
    fn cocoa_to_quartz_y_on_display_below_primary() {
        // Negative cocoa y means below primary; quartz y > primary_height.
        assert_eq!(cocoa_to_quartz_y(-300.0, 900.0), 1200.0);
    }

    #[test]
    fn observer_tick_returns_workspace_tick() {
        let mut observer = WorkspaceObserver::new();
        let tick = observer.tick(std::time::Instant::now());
        assert!(!tick.trust_changed, "first tick has no prior state to compare");
        // On the stub (non-macOS) build, workspace_available is false.
        // On macOS, this test runs but real polling hasn't been wired yet,
        // so workspace_available is also false. Both are acceptable.
        let _ = tick.snapshot;
    }

    #[test]
    fn observer_owns_borrow_releases_after_tick() {
        // Compile-fence test: if tick() returned &WorkspaceSnapshot, the second
        // borrow below would fail.
        let mut observer = WorkspaceObserver::new();
        let tick = observer.tick(std::time::Instant::now());
        let _trusted = observer.is_accessibility_trusted();
        let _ = tick;
    }

    #[test]
    fn editor_matcher_exact() {
        assert!(is_editor_bundle_id("com.apple.dt.Xcode"));
        assert!(is_editor_bundle_id("com.microsoft.VSCode"));
    }

    #[test]
    fn editor_matcher_jetbrains_prefix() {
        assert!(is_editor_bundle_id("com.jetbrains.intellij"));
        assert!(is_editor_bundle_id("com.jetbrains.RustRover"));
    }

    #[test]
    fn editor_matcher_rejects_unrelated_substring() {
        assert!(!is_editor_bundle_id("com.example.notxcode"));
        assert!(!is_editor_bundle_id(""));
    }

    #[test]
    fn startup_prompt_is_no_op_when_disabled() {
        let mut o = WorkspaceObserver::new();
        o.request_accessibility_on_startup_if_enabled(false);
        // Verify the gate flag does not flip (no AX call made).
        assert!(!o.prompted_for_accessibility_at_startup);
    }

    #[test]
    fn startup_prompt_flips_flag_after_first_call_when_enabled() {
        let mut o = WorkspaceObserver::new();
        o.request_accessibility_on_startup_if_enabled(true);
        assert!(o.prompted_for_accessibility_at_startup);
        // Second call returns immediately (the latch is set).
        o.request_accessibility_on_startup_if_enabled(true);
        assert!(o.prompted_for_accessibility_at_startup);
    }
}
