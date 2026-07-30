//! Arguments a key sequence can carry to the command it resolves to.
//!
//! A keymap that only maps *fixed* key sequences to command ids cannot express
//! an operator grammar: `d2w` would need one binding and one command id per
//! count, `f{char}` would need one per character, and the product of
//! operator × motion × text-object × count is unbounded. Both missing pieces are
//! carried here instead of being enumerated as data:
//!
//! - a **count**, accumulated from decimal digits typed where a binding declares
//!   it accepts one ([`KeyBinding::with_count_prefix`](super::KeyBinding::with_count_prefix));
//! - **captured characters**, one per
//!   [`StrokeCapture::AnyChar`](super::StrokeCapture::AnyChar) stroke in the
//!   matched sequence, in the order the strokes appear — which is what makes
//!   `f{char}`, `r{char}`, `m{mark}`, `"{register}` and `di{delim}` a single
//!   binding each.
//!
//! Arguments are **advisory**: a command that ignores them behaves exactly as it
//! did before, so the non-modal default keymap (which declares no counts and no
//! captures) always produces [`CommandArgs::NONE`].

/// The count and captured characters a matched key sequence carried.
///
/// Cheap to clone: the capture buffer holds at most one `char` per capture
/// stroke in the binding, and is empty for every binding in the default keymap.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandArgs {
    /// The numeric prefix the user typed, or `None` when none was.
    count: Option<u32>,
    /// Characters captured by wildcard strokes, in sequence order.
    captures: Vec<char>,
}

impl CommandArgs {
    /// No count and no captures: what every argument-free binding produces.
    pub const NONE: Self = Self {
        count: None,
        captures: Vec::new(),
    };

    /// Builds arguments from a count and a capture list.
    #[must_use]
    pub const fn new(count: Option<u32>, captures: Vec<char>) -> Self {
        Self { count, captures }
    }

    /// Builds arguments carrying only a count.
    #[must_use]
    pub const fn with_count(count: u32) -> Self {
        Self {
            count: Some(count),
            captures: Vec::new(),
        }
    }

    /// Builds arguments carrying only captured characters.
    #[must_use]
    pub const fn with_captures(captures: Vec<char>) -> Self {
        Self {
            count: None,
            captures,
        }
    }

    /// The numeric prefix the user typed, or `None`.
    #[must_use]
    pub const fn count(&self) -> Option<u32> {
        self.count
    }

    /// The count, or `1` when none was typed.
    ///
    /// The convention every repeat-capable command wants: `dw` deletes one word,
    /// `3dw` deletes three.
    #[must_use]
    pub const fn repeat_count(&self) -> u32 {
        match self.count {
            Some(count) => count,
            None => 1,
        }
    }

    /// The characters captured by wildcard strokes, in sequence order.
    #[must_use]
    pub fn captures(&self) -> &[char] {
        &self.captures
    }

    /// The `index`-th captured character, if the sequence captured that many.
    #[must_use]
    pub fn capture(&self, index: usize) -> Option<char> {
        self.captures.get(index).copied()
    }

    /// Returns `true` when neither a count nor a capture was carried.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count.is_none() && self.captures.is_empty()
    }
}

/// One resolved command together with the arguments its sequence carried.
///
/// [`Resolution::Matched`](super::Resolution) yields this rather than a bare
/// [`CommandId`](super::CommandId) so that an operator grammar — where the *same*
/// command id runs with a different count or a different captured character — is
/// expressible without enumerating the product as bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInvocation {
    /// The command to run.
    command: super::CommandId,
    /// The count and captures the key sequence carried.
    args: CommandArgs,
}

impl CommandInvocation {
    /// Builds an invocation.
    #[must_use]
    pub const fn new(command: super::CommandId, args: CommandArgs) -> Self {
        Self { command, args }
    }

    /// Builds an argument-free invocation.
    #[must_use]
    pub const fn bare(command: super::CommandId) -> Self {
        Self {
            command,
            args: CommandArgs::NONE,
        }
    }

    /// The command to run.
    #[must_use]
    pub const fn command(&self) -> &super::CommandId {
        &self.command
    }

    /// The arguments the key sequence carried.
    #[must_use]
    pub const fn args(&self) -> &CommandArgs {
        &self.args
    }

    /// Splits into the command and its arguments.
    #[must_use]
    pub fn into_parts(self) -> (super::CommandId, CommandArgs) {
        (self.command, self.args)
    }
}
