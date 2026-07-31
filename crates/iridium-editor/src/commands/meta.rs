//! Descriptive metadata for a registered command.

use std::borrow::Cow;

use serde::{Deserialize, Deserializer, Serialize, de};

use super::{CommandCategory, CommandId};

/// Rejects any attempt to supply aliases through `serde`.
///
/// [`CommandMeta::aliases`] is `&'static [&'static str]`, so there is nothing a
/// borrowed or owned deserialized string can become. The obvious spelling —
/// `skip_deserializing` — would make `"aliases": ["fmt"]` in a host's command
/// manifest *silently* do nothing, and a synonym that quietly never matches is
/// exactly the kind of defect a palette makes impossible to diagnose from the
/// outside. This turns that into a message naming the constructor to use.
///
/// An explicitly empty list is accepted, so a value produced by
/// [`Serialize`] with no aliases still round-trips.
fn deserialize_aliases<'de, D>(deserializer: D) -> Result<&'static [&'static str], D::Error>
where
    D: Deserializer<'de>,
{
    let supplied = Vec::<String>::deserialize(deserializer)?;
    if supplied.is_empty() {
        return Ok(&[]);
    }
    Err(de::Error::custom(
        "`aliases` cannot be deserialized: they must be `'static` literals compiled into the \
         kernel or the host, supplied with `CommandMeta::with_aliases`",
    ))
}

/// Whether a serialized [`CommandMeta`] should omit its alias list.
///
/// A free function rather than an inline path because the field is a *reference*
/// to a slice, and `serde` hands `skip_serializing_if` a reference to the field.
const fn aliases_are_empty(aliases: &&'static [&'static str]) -> bool {
    aliases.is_empty()
}

/// Everything the editor knows about one command *except* how to run it.
///
/// A command's behaviour stays in the input layer; this type carries only the
/// identity and the human-facing description the command palette, a
/// keybinding-help view, and a scripting host need in order to find, list and
/// explain the command without executing it.
///
/// Titles and descriptions are [`Cow<'static, str>`](Cow) so kernel commands
/// cost nothing to declare while hosts can register commands with computed
/// strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandMeta {
    /// The stable machine id.
    id: CommandId,
    /// Human-facing title, as shown in the palette (`"Duplicate Line Up"`).
    title: Cow<'static, str>,
    /// Optional longer explanation, shown as palette secondary text or help.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<Cow<'static, str>>,
    /// Palette grouping.
    category: CommandCategory,
    /// Advisory: `true` when running the command can change document text.
    mutates_document: bool,
    /// Extra search terms for the palette; see [`Self::with_aliases`].
    #[serde(
        default,
        deserialize_with = "deserialize_aliases",
        skip_serializing_if = "aliases_are_empty"
    )]
    aliases: &'static [&'static str],
}

impl CommandMeta {
    /// Declares a command with an id, a palette title and a category.
    ///
    /// The command is assumed **not** to mutate document text; call
    /// [`Self::mutating`] for those. Use [`Self::with_description`] to add the
    /// longer palette description.
    #[must_use]
    pub fn new(
        id: CommandId,
        title: impl Into<Cow<'static, str>>,
        category: CommandCategory,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            description: None,
            category,
            mutates_document: false,
            aliases: &[],
        }
    }

    /// Declares a command from string literals, in a `const` context.
    ///
    /// The constructor the built-in table uses, so the table can be a `static`
    /// slice rather than a `Vec` built at run time — which is what lets
    /// [`BUILTIN_COMMAND_COUNT`](crate::commands::builtin::BUILTIN_COMMAND_COUNT)
    /// be derived from the table instead of maintained alongside it.
    #[must_use]
    pub const fn from_static(
        id: CommandId,
        title: &'static str,
        category: CommandCategory,
    ) -> Self {
        Self {
            id,
            title: Cow::Borrowed(title),
            description: None,
            category,
            mutates_document: false,
            aliases: &[],
        }
    }

    /// Attaches a longer description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<Cow<'static, str>>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Declares a described command from string literals, in a `const` context.
    ///
    /// The description is a separate parameter rather than a builder step because
    /// replacing an `Option<Cow<'_, str>>` field would drop the old value, and a
    /// `const fn` may not run a destructor.
    #[must_use]
    pub const fn described(
        id: CommandId,
        title: &'static str,
        description: &'static str,
        category: CommandCategory,
    ) -> Self {
        Self {
            id,
            title: Cow::Borrowed(title),
            description: Some(Cow::Borrowed(description)),
            category,
            mutates_document: false,
            aliases: &[],
        }
    }

    /// Attaches extra search terms, in a `const` context.
    ///
    /// A palette matches a query against the title, the id, the category and the
    /// description — which between them cover most commands, and miss precisely
    /// the words a user reaches for that the author did not write down: `dupe`
    /// for *Duplicate Line*, `eol` for *Cursor to Line End*, `yank` for *Copy*.
    /// An alias earns its place by contributing a term a user would plausibly
    /// type that none of those four already offers; restating the title in other
    /// words only re-scores a command the query had already found.
    ///
    /// Aliases are `&'static [&'static str]` rather than owned strings for two
    /// reasons: they are always authored literals, and an owning collection has
    /// drop glue, which would keep the built-in table from being a `static`.
    /// A host registering a command at run time therefore supplies aliases from
    /// its own literals or leaves them empty. They are also **write-only over
    /// `serde`** — see [`deserialize_aliases`].
    ///
    /// Duplicates across commands are allowed and expected: `erase` genuinely
    /// belongs to both *Delete Backward* and *Delete Forward*. The palette's sort
    /// is total, so a query matching two commands equally still orders them
    /// deterministically.
    #[must_use]
    pub const fn with_aliases(mut self, aliases: &'static [&'static str]) -> Self {
        self.aliases = aliases;
        self
    }

    /// Marks the command as one that can change document text.
    ///
    /// This flag is **advisory metadata**, for greying out palette entries in a
    /// read-only buffer and for letting a scripting host classify a command
    /// without running it. It is deliberately *not* the authority for read-only
    /// enforcement: the authority remains the [`Command`](crate::history::Command)
    /// the handler actually produces, which the editor inspects before applying
    /// it. Two sources of truth are acceptable here only because the flag can
    /// never grant a mutation the produced command does not perform.
    #[must_use]
    pub const fn mutating(mut self) -> Self {
        self.mutates_document = true;
        self
    }

    /// The stable machine id.
    #[must_use]
    pub const fn id(&self) -> &CommandId {
        &self.id
    }

    /// The human-facing palette title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The optional longer description.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Extra palette search terms; empty for most commands.
    #[must_use]
    pub const fn aliases(&self) -> &'static [&'static str] {
        self.aliases
    }

    /// The palette grouping.
    #[must_use]
    pub const fn category(&self) -> &CommandCategory {
        &self.category
    }

    /// Whether running the command can change document text (advisory; see
    /// [`Self::mutating`]).
    #[must_use]
    pub const fn mutates_document(&self) -> bool {
        self.mutates_document
    }
}
