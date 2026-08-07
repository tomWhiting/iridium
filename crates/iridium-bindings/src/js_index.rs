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

/// Narrows an index on its way *out* to JavaScript.
///
/// The counterpart to [`index`], and the reason it exists is the same: the
/// obvious spelling is wrong in a way nothing reports. `wasm.rs` returned
/// `u32` from a dozen exports by writing `value as u32`, and clippy called
/// every one of them `cast_possible_truncation`.
///
/// # Why not just keep the cast
///
/// The cast is in fact sound today, and for a reason that is easy to state and
/// easy to stop being true: `wasm.rs` compiles **only** for `wasm32`, where
/// `usize` is 32 bits, so `usize as u32` cannot lose anything. Clippy's own
/// message says the truncation is a hazard "on targets with 64-bit wide
/// pointers", which that file never has.
///
/// ⭐ But that soundness lives in a fact about the *target*, held nowhere and
/// checked by nothing. Suppressing the lint would have recorded the argument
/// in a comment and left the code depending on it — and `wasm64` is a real
/// target. Writing it as a `try_from` instead needs **no** assumption about
/// pointer width: it is correct on every target, so there is nothing left to
/// hold true.
///
/// # Why saturate rather than fail
///
/// These are line numbers, column numbers and line counts on the way to a
/// display. On `wasm32` the fallback is unreachable — `usize::MAX` *is*
/// `u32::MAX` there — so this is about what the code says, not what it does.
/// Saturating keeps the function total and keeps the value monotone in the
/// input, where a wrap would make a very large document report a very small
/// one.
#[must_use]
pub fn to_js_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Narrows an index to the `i32` an export uses when `-1` means "none".
///
/// Separate from [`to_js_u32`] because the sentinel changes the question.
/// `get_fold_end_line` returns `-1` for "no fold region here", so a value that
/// wrapped into the negatives would not merely be wrong, it would be
/// *indistinguishable from the absence of a fold* — and `usize::MAX` wraps to
/// exactly `-1`.
///
/// ⚠️ This is the one cast in `wasm.rs` whose lint named the target we actually
/// build for. `cast_possible_wrap` warns about "targets with 32-bit wide
/// pointers", and `wasm32` is one: there `usize` is `u32`, and any value at or
/// above 2^31 comes out negative. The 2^31 lines that would take are not
/// reachable in a browser tab, but "not reachable" was a fact about document
/// sizes rather than about this function, which is exactly the shape worth
/// removing.
///
/// Saturating at [`i32::MAX`] keeps a fold that is too large to express
/// distinguishable from no fold at all.
#[must_use]
pub fn to_js_i32(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{index, to_js_i32, to_js_u32};

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

    #[test]
    fn an_ordinary_index_goes_out_unchanged() {
        assert_eq!(to_js_u32(0), 0);
        assert_eq!(to_js_u32(1), 1);
        assert_eq!(to_js_u32(4_294_967_295), u32::MAX);
        assert_eq!(to_js_i32(0), 0);
        assert_eq!(to_js_i32(2_147_483_647), i32::MAX);
    }

    /// The values a bare `as` would have wrapped, and what it would have made
    /// of them.
    ///
    /// These only *occur* on a 64-bit host, which is exactly why the test can
    /// run here and the code it protects cannot. On `wasm32` the inputs below
    /// are unrepresentable; the point is that the function no longer depends on
    /// that being so.
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn a_value_too_large_saturates_rather_than_wrapping() {
        // `usize::MAX as u32` is 0xFFFF_FFFF — u32::MAX, which is also what
        // saturating gives, so u32's interesting case is a value just past the
        // boundary rather than the maximum.
        assert_eq!(to_js_u32(4_294_967_296), u32::MAX, "2^32 must not become 0");
        assert_eq!(to_js_u32(usize::MAX), u32::MAX);
    }

    /// The sentinel collision, stated as a test.
    ///
    /// `get_fold_end_line` returns `-1` for "no fold region here". A bare
    /// `usize::MAX as i32` is exactly `-1`, so the largest possible fold end
    /// would have been reported as *no fold at all* — a wrong answer wearing
    /// the shape of a legitimate one.
    #[cfg(target_pointer_width = "64")]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "the discarded cast is reproduced deliberately, so the test \
                  states the defect rather than describing it"
    )]
    #[test]
    fn a_saturated_fold_end_is_never_the_no_fold_sentinel() {
        assert_eq!(
            usize::MAX as i32,
            -1,
            "the premise: this is what the code used to do"
        );
        assert_eq!(
            to_js_i32(usize::MAX),
            i32::MAX,
            "a fold too large to express must stay distinguishable from no fold"
        );
        assert_ne!(to_js_i32(usize::MAX), -1);
        assert_eq!(
            to_js_i32(2_147_483_648),
            i32::MAX,
            "2^31 must not go negative"
        );
    }
}
