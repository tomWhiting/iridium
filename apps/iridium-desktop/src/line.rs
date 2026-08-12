//! The shared row arithmetic, under the path this face has always used.
//!
//! The code moved to [`iridium_panel::line`] when the file explorer had to
//! reach the terminal face; the module path stays so that every builder here
//! reads exactly as it did, and so that one import line is the whole record of
//! where it went.

pub use iridium_panel::line::{LineBuilder, highlighted_spans, match_color, skip_chars};
