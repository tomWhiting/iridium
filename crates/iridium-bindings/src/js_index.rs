//! Turning a JavaScript number into an index, and refusing the ones that are
//! not.
//!
//! JavaScript has one numeric type, and every line number, byte offset and
//! array index that crosses the boundary arrives as an `f64`. Rust's `as` will
//! narrow any of them to a `usize` without complaint, and that is the problem:
//! `as` **saturates**. `-1.0 as usize` is `0`. `f64::NAN as usize` is `0`.
//! `1e30 as usize` is `usize::MAX`.
//!
//! So a host that computed a line number wrongly — an off-by-one that went
//! negative, an arithmetic slip that produced `NaN` — did not get an error. It
//! got its highlight, its blame line or its change bar applied to **line
//! zero**, which is a real line, visibly wrong, and attributable to nothing.
//! Every call site already returned a `Result` and already reported a
//! non-number; the error channel was there and range simply never used it.
//!
//! # Why this is a plain function
//!
//! The call sites are `#[wasm_bindgen]` methods in `wasm.rs`, which compile
//! only for `wasm32` and so are reachable by no native test. The *policy* —
//! which numbers are indices and which are mistakes — is the part worth
//! testing, so it lives here, compiles everywhere, and the wasm layer is left
//! holding nothing but the `JsValue` plumbing.

/// The largest integer an `f64` can hold exactly: 2^53.
///
/// Past this, consecutive `f64` values are more than one apart, so an index
/// above it did not survive the trip from JavaScript intact whatever this
/// function does with it. It is roughly nine quadrillion — some fourteen
/// orders of magnitude beyond any line count or byte offset a document could
/// have — so rejecting above it costs nothing real.
const MAX_EXACT: f64 = 9_007_199_254_740_992.0;

/// Reads `value` as an index, or says why it is not one.
///
/// `field` names the property the number came from, so the message points at
/// the caller's own key rather than at a position in an argument list.
///
/// Accepts only a non-negative whole number that an `f64` represents exactly
/// and a `usize` can hold. `NaN` and both infinities fail the range check
/// (nothing compares true against `NaN`), a negative fails it, and a
/// fractional value fails on its own terms — `2.5` is not a line, and rounding
/// it would be inventing an answer the caller did not give.
///
/// # Errors
///
/// A human-readable message naming `field` and what was wrong with it.
pub fn index(value: f64, field: &str) -> Result<usize, String> {
    if !(0.0..=MAX_EXACT).contains(&value) {
        return Err(format!(
            "{field} must be a whole number between 0 and {MAX_EXACT}, and was {value}"
        ));
    }
    if value.fract() != 0.0 {
        return Err(format!("{field} must be a whole number, and was {value}"));
    }

    // Exact: the guards above leave only a non-negative integral `f64` at most
    // 2^53, every one of which `u64` holds precisely.
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "the range and fract guards immediately above admit only \
                  non-negative integers below 2^53"
    )]
    let whole = value as u64;

    usize::try_from(whole)
        .map_err(|_| format!("{field} is {whole}, which does not fit this platform's index type"))
}

#[cfg(test)]
mod tests {
    use super::index;

    #[test]
    fn a_whole_non_negative_number_is_an_index() {
        assert_eq!(index(0.0, "line"), Ok(0));
        assert_eq!(index(1.0, "line"), Ok(1));
        assert_eq!(index(4_294_967_295.0, "start"), Ok(4_294_967_295));
    }

    /// The four values `as usize` would have turned into a real index.
    ///
    /// This is the whole point of the module. Each of these used to become a
    /// number the caller never wrote: the first three `0`, the last
    /// `usize::MAX`.
    #[test]
    fn the_values_a_bare_cast_would_have_swallowed_are_refused() {
        for (value, name) in [
            (-1.0, "a negative line"),
            (f64::NAN, "NaN"),
            (f64::NEG_INFINITY, "negative infinity"),
            (1e30, "a value past the exact-integer limit"),
        ] {
            let outcome = index(value, "line");
            assert!(
                outcome.is_err(),
                "{name} was accepted as {outcome:?}, and a bare cast would have \
                 made it a real line"
            );
        }
    }

    #[test]
    fn positive_infinity_is_refused_too() {
        assert!(index(f64::INFINITY, "end").is_err());
    }

    /// A fraction is a mistake, not something to round.
    ///
    /// Rounding would answer a question the caller did not ask, and the two
    /// plausible roundings — down and to-nearest — disagree at exactly the
    /// values a caller is most likely to produce by accident.
    #[test]
    fn a_fraction_is_refused_rather_than_rounded() {
        assert!(index(2.5, "line").is_err());
        assert!(index(0.5, "line").is_err());
        assert!(index(-0.5, "line").is_err());
    }

    #[test]
    fn the_message_names_the_field_the_caller_used() {
        let message = index(-1.0, "startColumn").expect_err("a negative is refused");
        assert!(
            message.contains("startColumn"),
            "the message must point at the caller's own key: {message}"
        );
        assert!(
            message.contains("-1"),
            "and at the value it held: {message}"
        );
    }

    /// Negative zero is zero, and IEEE 754 says so.
    ///
    /// Worth pinning because it is the one value where "is it negative?" and
    /// "is it less than zero?" give different answers, and a sign-bit check
    /// would have rejected a perfectly good index.
    #[test]
    fn negative_zero_is_zero() {
        assert_eq!(index(-0.0, "line"), Ok(0));
    }
}
