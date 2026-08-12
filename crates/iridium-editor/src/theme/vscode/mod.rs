//! VS Code theme compatibility — reading a `.json` colour theme as a [`Theme`].
//!
//! VS Code themes are the largest body of editor themes in existence, and
//! importing one should produce the theme its author drew rather than a
//! partial rendering of it.
//!
//! # The three pieces
//!
//! - [`document`] — the JSON shapes as VS Code writes them, and the conversion
//!   into a [`Theme`].
//! - [`scope`] — one `TextMate` scope → [`HighlightType`](crate::HighlightType)
//!   table, which answers both the colour and the emphasis question so the two
//!   cannot disagree about which scopes count as a comment.
//! - [`font_style`] — `fontStyle`, including the empty string that means
//!   *explicitly regular* and the two styles Iridium cannot draw.
//!
//! # ⚠️ What an import does not carry
//!
//! Stated here rather than discovered: `background` on a token rule (Iridium
//! draws one editor background, not per-token ones), `underline` and
//! `strikethrough` (quad geometry, not font attributes — see [`font_style`]),
//! and every `TextMate` scope no row in [`scope`] claims. The last is the large
//! one and it is deliberately unchanged by the work that made `fontStyle`
//! arrive: broadening scope coverage moves *colours*, and mixing that into a
//! commit about style would make each change the suspect for the other.
//!
//! [`Theme`]: crate::theme::Theme

mod document;
mod font_style;
mod scope;

pub use document::{VsCodeTheme, VsCodeTokenColor, convert_vscode_theme};
