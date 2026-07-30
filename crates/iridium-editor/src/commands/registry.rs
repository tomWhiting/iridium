//! The command registry: the single source of truth for what the editor can do.

use std::collections::HashMap;

use super::{CommandId, CommandMeta, RegistryError};

/// An append-only table of every command the editor exposes.
///
/// The registry is what turns editor actions from branches in a `match`
/// statement into **named, described, addressable values**. Three consumers read
/// it:
///
/// - a **keymap** maps key sequences to [`CommandId`]s and needs `O(1)` lookup
///   on the keystroke hot path;
/// - the **command palette** enumerates it and needs a *stable* order so search
///   results do not jitter between runs;
/// - a **scripting or AI host** enumerates it to discover the editor's
///   vocabulary, and must do so through this public surface rather than from
///   inside the kernel.
///
/// # Ordering guarantees
///
/// The registry never exposes [`HashMap`] iteration order. Two deterministic
/// orders are offered instead:
///
/// - [`Self::commands`] yields commands in **registration order**, which is the
///   order [`register_builtin_commands`](super::builtin::register_builtin_commands)
///   declares them and therefore fixed at compile time.
/// - [`Self::palette_order`] yields commands sorted by
///   (category, title, id) — total and independent of registration order, so a
///   host that registers commands in a data-dependent order still gets a stable
///   palette.
#[derive(Debug, Clone, Default)]
pub struct CommandRegistry {
    /// Commands in registration order.
    entries: Vec<CommandMeta>,
    /// Index from id to position in [`Self::entries`].
    by_id: HashMap<CommandId, usize>,
}

impl CommandRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            by_id: HashMap::new(),
        }
    }

    /// Creates an empty registry with room for `capacity` commands.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            by_id: HashMap::with_capacity(capacity),
        }
    }

    /// Registers one command.
    ///
    /// # Errors
    ///
    /// - [`RegistryError::EmptyId`] if the command's id is empty.
    /// - [`RegistryError::DuplicateId`] if the id is already registered.
    ///   Registration never overwrites; the first registration wins and the
    ///   caller learns about the collision.
    pub fn register(&mut self, meta: CommandMeta) -> Result<(), RegistryError> {
        if meta.id().is_empty() {
            return Err(RegistryError::EmptyId);
        }
        if self.by_id.contains_key(meta.id().as_str()) {
            return Err(RegistryError::DuplicateId {
                id: meta.id().as_str().to_owned(),
            });
        }
        let index = self.entries.len();
        self.by_id.insert(meta.id().clone(), index);
        self.entries.push(meta);
        Ok(())
    }

    /// Registers every command in `metas`, stopping at the first failure.
    ///
    /// # Errors
    ///
    /// Propagates the first [`RegistryError`]. Commands before the failure stay
    /// registered; a caller that needs all-or-nothing should build a fresh
    /// registry and swap it in on success.
    pub fn register_all(
        &mut self,
        metas: impl IntoIterator<Item = CommandMeta>,
    ) -> Result<(), RegistryError> {
        for meta in metas {
            self.register(meta)?;
        }
        Ok(())
    }

    /// Looks up a command by id.
    ///
    /// A single hash probe against the borrowed id text; accepts anything that
    /// derefs to `&str`, so no [`CommandId`] needs to be constructed (and
    /// nothing is allocated) to answer a lookup on the keystroke hot path.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&CommandMeta> {
        self.by_id
            .get(id)
            .and_then(|&index| self.entries.get(index))
    }

    /// Returns `true` when `id` is registered.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.by_id.contains_key(id)
    }

    /// Returns the registry's own [`CommandId`] instance for `id`.
    ///
    /// Used by [`Keymap::canonicalize`](super::Keymap::canonicalize) to replace a
    /// deserialized keymap's owned ids with the registry's `'static` ones, which
    /// makes subsequent resolution allocation free.
    #[must_use]
    pub fn canonical_id(&self, id: &str) -> Option<&CommandId> {
        self.get(id).map(CommandMeta::id)
    }

    /// The number of registered commands.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when no commands are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Enumerates commands in registration order.
    ///
    /// Deterministic across runs and processes.
    pub fn commands(&self) -> impl ExactSizeIterator<Item = &CommandMeta> {
        self.entries.iter()
    }

    /// Enumerates commands sorted by (category, title, id) for palette display.
    ///
    /// A total order over distinct commands — ids are unique, so ties are
    /// impossible — which makes palette ordering independent of the order
    /// commands happened to be registered in.
    #[must_use]
    pub fn palette_order(&self) -> Vec<&CommandMeta> {
        let mut ordered: Vec<&CommandMeta> = self.entries.iter().collect();
        ordered.sort_by(|left, right| {
            left.category()
                .cmp(right.category())
                .then_with(|| left.title().cmp(right.title()))
                .then_with(|| left.id().cmp(right.id()))
        });
        ordered
    }

    /// Enumerates the commands of one category, in palette order.
    #[must_use]
    pub fn in_category(&self, category: &str) -> Vec<&CommandMeta> {
        self.palette_order()
            .into_iter()
            .filter(|meta| meta.category().as_str() == category)
            .collect()
    }
}
