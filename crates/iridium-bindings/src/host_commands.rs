//! The host command ids, exported so a face never re-types one.
//!
//! Deliberately **not** gated on `feature = "web"`, for the reason
//! [`crate::palette`] is not: everything here is a plain function over kernel
//! constants, so `cargo test` exercises it on the host target and only the
//! three-line `#[wasm_bindgen]` adapter in `wasm.rs` is browser-only.
//!
//! # Why this exists at all
//!
//! `iridium_editor::commands::builtin::host` states the whole argument for
//! host commands: the kernel cannot draw a UI, but it *owns the identity* of
//! the action, so one id and one key sequence mean the same thing in every
//! face. Every Rust face honours that by naming the constants — the desktop
//! shell's `dispatch_host_command`, the terminal face's, the workspace's own
//! `run_command`. Rename a constant and each of them fails to compile.
//!
//! **TypeScript had no such link.** The web component compared
//! `request.command === "palette.open"` against a literal it had typed out
//! itself, and nothing anywhere checked the two spellings still agreed. The
//! divergence case is a rename: every Rust face stops compiling, while the
//! browser face compiles, type-checks, passes its whole suite, and quietly
//! stops opening the palette — a chord that is consumed and does nothing,
//! which is the hardest kind of dead key to trace.
//!
//! So the ids cross the boundary as data. A face reads them once at
//! creation and compares against what the kernel actually named.
//!
//! # Why named fields rather than a list
//!
//! A face does not want *the ids*; it wants "the one that means open the
//! palette". A bare list would push the naming back into TypeScript, which is
//! the transcription this module removes. The fields are the names, and
//! [`tests::every_host_command_the_kernel_names_is_exported`] is the oracle
//! that keeps them level with the kernel: adding a host command without
//! exporting it fails that test rather than shipping a browser that cannot
//! reach it.

use iridium_editor::commands::builtin::{
    EXPLORER_TOGGLE_PANEL, HISTORY_TOGGLE_PANEL, PALETTE_OPEN,
};
use serde::{Deserialize, Serialize};

/// Every host command id the kernel names, under the name a face knows it by.
///
/// `camelCase` on the wire; the field names are the TypeScript surface, and
/// `the_ids_serialize_under_the_names_typescript_reads` pins them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostCommandIds {
    /// Open the command palette — [`PALETTE_OPEN`].
    pub palette_open: String,
    /// Show or hide the undo-tree panel — [`HISTORY_TOGGLE_PANEL`].
    pub history_toggle_panel: String,
    /// Show or hide the file explorer — [`EXPLORER_TOGGLE_PANEL`].
    ///
    /// Exported even though a browser has no directory to show. A face that
    /// cannot implement one still has to *recognise* it: the kernel's default
    /// keymap binds `Ctrl+Alt+E`, so the chord is consumed either way, and a
    /// face that cannot name the command cannot tell the user why nothing
    /// happened.
    pub explorer_toggle_panel: String,
}

/// The host command ids, read from the kernel's own constants.
///
/// Allocates three short strings, and is called once per editor rather than
/// per keystroke.
#[must_use]
pub fn host_command_ids() -> HostCommandIds {
    HostCommandIds {
        palette_open: PALETTE_OPEN.as_str().to_owned(),
        history_toggle_panel: HISTORY_TOGGLE_PANEL.as_str().to_owned(),
        explorer_toggle_panel: EXPLORER_TOGGLE_PANEL.as_str().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{HostCommandIds, host_command_ids};

    use iridium_editor::commands::builtin::HOST;

    use std::collections::BTreeSet;

    /// The exported ids as a set, so the comparison below is order-free.
    fn exported(ids: &HostCommandIds) -> BTreeSet<&str> {
        BTreeSet::from([
            ids.palette_open.as_str(),
            ids.history_toggle_panel.as_str(),
            ids.explorer_toggle_panel.as_str(),
        ])
    }

    /// The oracle: what this module exports is exactly what the kernel names.
    ///
    /// Set equality rather than a count, so it fails in both directions — a
    /// host command added to the kernel and not exported here (a browser that
    /// cannot recognise a chord the kernel consumes), and a field left behind
    /// after a command was retired (a face comparing against an id nothing
    /// will ever send).
    ///
    /// It also catches two fields carrying the same id, without a separate
    /// test: duplicates collapse in the set, so three fields holding two
    /// distinct ids can never equal a three-element kernel table.
    #[test]
    fn every_host_command_the_kernel_names_is_exported() {
        let ids = host_command_ids();
        let named: BTreeSet<&str> = HOST.iter().map(|meta| meta.id().as_str()).collect();

        assert_eq!(
            exported(&ids),
            named,
            "the exported host command ids and the kernel's `HOST` table have diverged; \
             add or remove a field on `HostCommandIds` and the TypeScript that reads it"
        );
    }

    /// The wire names TypeScript destructures, pinned.
    ///
    /// This is also what makes the `{}` fallback in `wasm.rs`'s adapter
    /// unreachable rather than merely unlikely: a struct of three `String`s
    /// has no value `serde_json` can refuse.
    #[test]
    fn the_ids_serialize_under_the_names_typescript_reads() {
        let json = serde_json::to_string(&host_command_ids()).expect("three strings");

        assert_eq!(
            json,
            r#"{"paletteOpen":"palette.open","historyTogglePanel":"history.togglePanel","explorerTogglePanel":"explorer.togglePanel"}"#
        );
    }
}
