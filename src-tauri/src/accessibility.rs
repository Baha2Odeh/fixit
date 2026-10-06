//! macOS Accessibility permission. Fixit needs it to send Cmd+C / Cmd+V to other
//! apps. Other platforms don't gate this, so everything there reports "granted".

use tauri::AppHandle;

#[cfg(target_os = "macos")]
mod mac {
    use core_foundation::{
        base::TCFType, boolean::CFBoolean, dictionary::CFDictionary, string::CFString,
    };

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> bool;
    }

    /// Checks the permission without showing anything.
    pub fn is_trusted() -> bool {
        // SAFETY: plain C function with no arguments.
        unsafe { AXIsProcessTrusted() }
    }

    /// Checks the permission and, if missing, shows the system dialog that
    /// offers to open System Settings > Privacy & Security > Accessibility.
    pub fn is_trusted_with_prompt() -> bool {
        // This is the value of the kAXTrustedCheckOptionPrompt constant.
        let key = CFString::new("AXTrustedCheckOptionPrompt");
        let options =
            CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
        // SAFETY: `options` is a valid CFDictionary that outlives the call.
        unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
    }
}

pub fn is_granted() -> bool {
    #[cfg(target_os = "macos")]
    return mac::is_trusted();
    #[cfg(not(target_os = "macos"))]
    return true;
}

/// Shows the system permission dialog if access is missing. Returns whether it
/// is already granted.
pub fn request() -> bool {
    #[cfg(target_os = "macos")]
    return mac::is_trusted_with_prompt();
    #[cfg(not(target_os = "macos"))]
    return true;
}

/// Opens the Accessibility page of System Settings (no-op elsewhere).
pub fn open_settings(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_opener::OpenerExt;
        let _ = app.opener().open_url(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            None::<&str>,
        );
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

pub const MISSING_MESSAGE: &str = "Fixit needs Accessibility permission to copy and paste. \
    Turn it on in System Settings > Privacy & Security > Accessibility, then try again.";
