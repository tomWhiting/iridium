//! `AppKit`'s file-open callback, retained for the lifetime of the winit loop.

use std::io::{self, Write};
use std::path::PathBuf;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSApplication, NSApplicationDelegate};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSURL};
use winit::event_loop::EventLoopProxy;

use crate::events::DesktopEvent;

define_class!(
    // SAFETY: NSObject has no subclassing requirements. AppKit invokes its
    // delegate on the main thread; the class has no custom Drop implementation.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "IridiumFileDelegate"]
    #[ivars = EventLoopProxy<DesktopEvent>]
    pub struct FileDelegate;

    unsafe impl NSObjectProtocol for FileDelegate {}

    unsafe impl NSApplicationDelegate for FileDelegate {
        #[unsafe(method(application:openURLs:))]
        fn open_urls(&self, _application: &NSApplication, urls: &NSArray<NSURL>) {
            let files = urls.iter().map(|url| file_path(&url)).collect();
            if let Err(error) = self.ivars().send_event(DesktopEvent::OpenFiles(files)) {
                // Closing the event loop is the only send failure. The editor
                // can no longer display it; stderr is the remaining channel.
                if writeln!(io::stderr(), "iridium-desktop: file delivery failed: {error}")
                    .is_err()
                {
                    // Both channels have closed during shutdown.
                    return;
                }
            }
        }
    }
);

impl FileDelegate {
    #[expect(
        unsafe_code,
        reason = "NSObject initialization after set_ivars is the objc2 subclass constructor contract"
    )]
    fn new(marker: MainThreadMarker, proxy: EventLoopProxy<DesktopEvent>) -> Retained<Self> {
        let this = Self::alloc(marker).set_ivars(proxy);
        // SAFETY: NSObject's initializer accepts this allocated subclass.
        unsafe { msg_send![super(this), init] }
    }
}

pub fn install(proxy: EventLoopProxy<DesktopEvent>) -> Result<Retained<FileDelegate>, String> {
    let marker = MainThreadMarker::new()
        .ok_or_else(|| "file-open delegate requires the main thread".to_owned())?;
    let app = NSApplication::sharedApplication(marker);
    if app.delegate().is_some() {
        return Err("an application delegate is already installed".to_owned());
    }
    let delegate = FileDelegate::new(marker, proxy);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    Ok(delegate)
}

fn file_path(url: &NSURL) -> Result<PathBuf, String> {
    if !url.isFileURL() {
        return Err("Iridium can only open local file URLs".to_owned());
    }
    url.to_file_path()
        .ok_or_else(|| "Finder supplied a file URL whose path could not be decoded".to_owned())
}

#[cfg(test)]
mod tests {
    use objc2_foundation::NSString;

    use super::*;

    #[test]
    fn file_urls_preserve_spaces_unicode_and_percent_characters() {
        let path = PathBuf::from("/tmp/Iridium café/100% ready.rs");
        let url = NSURL::from_file_path(&path).expect("file URL");
        assert_eq!(file_path(&url), Ok(path));
    }

    #[test]
    fn remote_urls_are_not_treated_as_local_paths() {
        let url = NSURL::URLWithString(&NSString::from_str("https://example.com/code.rs"))
            .expect("web URL");
        assert!(file_path(&url).is_err());
    }
}
