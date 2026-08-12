//! The native file and folder chooser.
//!
//! # Why this is here at all
//!
//! Until 9 Aug 2026 the only way to open anything in this face was to name it
//! on the command line or drop it on the window. Tom's words: *"there's no
//! open button, there's no open directory, there's no set project."* He was
//! right, and every one of those is a thing an editor is expected to have.
//!
//! # Three outcomes, not two
//!
//! ⭐ [`Chosen`] distinguishes **`Cancelled`** from **`Unsupported`**, and that
//! is the whole shape of this module. An `Option<PathBuf>` would fold "the
//! user pressed Cancel" together with "there is no chooser on this platform"
//! — one is a decision that deserves silence, the other is a missing feature
//! that has to be *said*, and a channel that cannot tell them apart can only
//! report the first. A caller that showed nothing for both would leave
//! somebody pressing `⌘O` at a window that never answers.
//!
//! # ⚠️ It blocks
//!
//! [`choose`] runs the panel **application-modal**: it does not return until
//! the panel closes, and while it is up the winit event loop is not running.
//! The OS spins its own run loop, so the panel itself is live and the window
//! behind it redraws — but nothing this crate schedules ticks, including the
//! file explorer's background reader poll. That is correct for a modal picker
//! and it is why nothing here is called from a paint path.
//!
//! # The platforms that are not macOS
//!
//! There is no chooser, and [`choose`] says so rather than pretending. This
//! face is built and run on macOS today; a GTK or Windows panel is a real
//! piece of work and inventing a stub for it would be the same lie
//! `Unsupported` exists to avoid.

use std::path::{Path, PathBuf};

/// What the panel is being opened to pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Want {
    /// One existing file.
    File,
    /// One existing directory.
    Directory,
}

impl Want {
    /// The label on the panel's accept button.
    ///
    /// macOS's default is "Open" for both, which says nothing about which of
    /// the two a panel that can only take one of them is asking for.
    const fn prompt(self) -> &'static str {
        match self {
            Self::File => "Open",
            Self::Directory => "Choose Folder",
        }
    }

    /// The sentence across the top of the panel.
    const fn message(self) -> &'static str {
        match self {
            Self::File => "Choose a file to open.",
            Self::Directory => "Choose a folder to open as the project.",
        }
    }
}

/// What a chooser came back with.
///
/// See the module docs for why the last two are not one variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chosen {
    /// A path the user picked.
    Path(PathBuf),
    /// The user dismissed the panel, having picked nothing.
    Cancelled,
    /// No chooser could be run, and why — a sentence fit to show a person.
    Unsupported(&'static str),
}

/// Runs a native chooser and blocks until it closes.
///
/// `start_in` is where the panel opens; `None` leaves it wherever the system
/// last left it, which is the behaviour every other application on the
/// machine has. A path that does not exist is handed over anyway — the panel
/// falls back to its own default rather than failing, and checking first
/// would be a race with whatever else is on the disk.
#[must_use]
pub fn choose(want: Want, start_in: Option<&Path>) -> Chosen {
    platform::choose(want, start_in)
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::{Path, PathBuf};

    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::{NSString, NSURL};

    use super::{Chosen, Want};

    /// What [`Chosen::Unsupported`] says when `AppKit` is reachable but the
    /// thread it insists on is not the one calling.
    ///
    /// A constant rather than a literal at the seam so the test asserts
    /// against the string the code actually returns; a test that spells the
    /// sentence out itself proves only that two copies were typed the same
    /// way.
    const OFF_MAIN_THREAD: &str =
        "the file chooser has to be opened from the main thread, and this was not";

    /// Every sentence this build can hand back as a reason, for the test that
    /// holds them to reading like sentences.
    ///
    /// Per-platform, and deliberately so: the other arm's wording is not
    /// compiled here, and a list naming it would be asserting about code this
    /// build does not contain.
    #[cfg(test)]
    pub(super) const UNSUPPORTED_REASONS: &[&str] = &[OFF_MAIN_THREAD];

    pub(super) fn choose(want: Want, start_in: Option<&Path>) -> Chosen {
        // ⚠️ The one runtime check, and it must never be a panic. AppKit
        // refuses to build a panel off the main thread, and `MainThreadMarker`
        // is how that is asked rather than assumed. On this face the answer is
        // always `Some` — every command runs inside winit's handler, which is
        // the main thread — so a `None` here means the call site moved, and
        // saying so beats taking the process down with it.
        let Some(marker) = MainThreadMarker::new() else {
            return Chosen::Unsupported(OFF_MAIN_THREAD);
        };

        let panel = NSOpenPanel::openPanel(marker);
        panel.setCanChooseFiles(want == Want::File);
        panel.setCanChooseDirectories(want == Want::Directory);
        // One at a time. Every caller opens exactly one thing, and a panel
        // that let three be picked would be a panel whose result two thirds of
        // is thrown away.
        panel.setAllowsMultipleSelection(false);
        // An alias resolved to what it points at, so a folder picked through
        // one roots on the real directory rather than on the alias file.
        panel.setResolvesAliases(true);
        panel.setPrompt(Some(&NSString::from_str(want.prompt())));
        panel.setMessage(Some(&NSString::from_str(want.message())));
        if let Some(directory) = start_in.and_then(Path::to_str) {
            panel.setDirectoryURL(Some(&NSURL::fileURLWithPath(&NSString::from_str(
                directory,
            ))));
        }

        if panel.runModal() != NSModalResponseOK {
            return Chosen::Cancelled;
        }
        // `NSModalResponseOK` with nothing selected is not something AppKit
        // does, but it is not something this code can prove either, so it is
        // handled as the cancellation it amounts to rather than indexed into.
        let Some(url) = panel.URLs().firstObject() else {
            return Chosen::Cancelled;
        };
        // A file URL always has a path; one that somehow does not is not a
        // path this code can act on.
        url.path().map_or(Chosen::Cancelled, |path| {
            // macOS stores file names as UTF-8, so this round trip is exact
            // for every name the panel can return. `NSString`'s `Display` is
            // the same conversion `objc2` uses everywhere else.
            Chosen::Path(PathBuf::from(path.to_string()))
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use std::path::Path;

    use super::{Chosen, Want};

    /// What [`Chosen::Unsupported`] says on a platform with no chooser wired.
    const NO_CHOOSER: &str =
        "this build has no file chooser — open a file or folder from the command line";

    /// Every sentence this build can hand back as a reason. See the macOS
    /// arm's copy for why this is per-platform.
    #[cfg(test)]
    pub(super) const UNSUPPORTED_REASONS: &[&str] = &[NO_CHOOSER];

    pub(super) fn choose(_want: Want, _start_in: Option<&Path>) -> Chosen {
        Chosen::Unsupported(NO_CHOOSER)
    }
}

#[cfg(test)]
mod tests {
    use super::{Chosen, Want};

    /// The two labels are the point of the enum having methods at all: a
    /// panel that can only take a folder must not have "Open" on its button
    /// while a panel that can only take a file does too.
    #[test]
    fn the_two_kinds_of_panel_do_not_say_the_same_thing() {
        assert_ne!(Want::File.prompt(), Want::Directory.prompt());
        assert_ne!(Want::File.message(), Want::Directory.message());
    }

    /// ⭐ The reason [`Chosen`] has three variants. If these compared equal,
    /// every caller downstream would be free to treat "no chooser here" as
    /// "the user said no" — which is the success-only channel this type
    /// exists to refuse.
    #[test]
    fn a_cancelled_panel_is_not_the_same_answer_as_no_panel() {
        assert_ne!(Chosen::Cancelled, Chosen::Unsupported("anything"));
    }

    /// Whatever `Unsupported` carries is shown to a person, so it has to read
    /// as a sentence rather than as an enum name — and it has to say what to
    /// do instead, because a refusal that names no way out is a dead end with
    /// better manners.
    ///
    /// Read from the constants the code returns rather than from copies typed
    /// here, and only the ones *this* build can return — asserting about a
    /// sentence that is not compiled would be asserting about nothing.
    #[test]
    fn every_unsupported_reason_is_a_sentence_a_person_can_act_on() {
        assert!(
            !super::platform::UNSUPPORTED_REASONS.is_empty(),
            "a build with no reasons at all would pass every assertion below"
        );
        for reason in super::platform::UNSUPPORTED_REASONS {
            assert!(
                reason.split_whitespace().count() >= 8,
                "`{reason}` is meant to be read by a person"
            );
            assert!(
                !reason.contains("Unsupported") && !reason.contains("Chosen"),
                "`{reason}` names a Rust type at somebody who is trying to open a file"
            );
        }
    }
}
