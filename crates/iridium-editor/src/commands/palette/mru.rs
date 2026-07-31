//! The recently-used command list that biases palette ranking.

use std::collections::VecDeque;

use crate::commands::CommandId;

/// How many commands are remembered.
///
/// Sixteen is about as far back as a user's sense of "I just used that" reaches;
/// beyond it the bias is noise, and the list is walked linearly on every scored
/// command, so it is deliberately small.
pub const MRU_CAPACITY: usize = 16;

/// The bonus given to the most recently used command.
const MRU_BONUS: i32 = 400;
/// How much of that bonus each step back in the list gives up.
const MRU_DECAY: i32 = 20;

/// The commands most recently invoked, newest first.
///
/// Lives in the kernel rather than in each face so that recency is shared: the
/// terminal, a native embedder and the browser all rank by the same history, and
/// none of them has to reimplement the rule.
///
/// The bias is the single largest perceived-quality lever a palette has, and it
/// is also the one most able to *hurt*: a strong bonus on a weak match puts the
/// wrong command first. The scale is chosen so recency decides between
/// comparable matches and loses to a clearly better one — see the ranking tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandMru {
    recent: VecDeque<CommandId>,
}

impl CommandMru {
    /// An empty history.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            recent: VecDeque::new(),
        }
    }

    /// Records `id` as the most recently used command.
    ///
    /// Moving an existing entry to the front rather than adding a second copy is
    /// what keeps the ranks meaningful: running one command ten times must not
    /// evict the other fifteen.
    pub fn record(&mut self, id: &CommandId) {
        if let Some(existing) = self
            .recent
            .iter()
            .position(|candidate| candidate.as_str() == id.as_str())
        {
            self.recent.remove(existing);
        }
        self.recent.push_front(id.clone());
        while self.recent.len() > MRU_CAPACITY {
            self.recent.pop_back();
        }
    }

    /// How recently `id` was used: `0` is the most recent, `None` never.
    #[must_use]
    pub fn rank(&self, id: &str) -> Option<usize> {
        self.recent
            .iter()
            .position(|candidate| candidate.as_str() == id)
    }

    /// The score bonus `id` earns for its recency, or `0` when it has none.
    ///
    /// Linear decay, so the most recent command leads the sixteenth by the width
    /// of the whole bonus minus one step, and every rank is distinct.
    #[must_use]
    pub fn bonus(&self, id: &str) -> i32 {
        self.rank(id).map_or(0, |rank| {
            MRU_BONUS - MRU_DECAY * i32::try_from(rank).unwrap_or(i32::MAX)
        })
    }

    /// The remembered commands, most recent first.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &CommandId> {
        self.recent.iter()
    }

    /// How many commands are remembered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.recent.len()
    }

    /// Whether nothing has been recorded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.recent.is_empty()
    }

    /// Forgets every recorded command.
    pub fn clear(&mut self) {
        self.recent.clear();
    }
}

impl<'a> IntoIterator for &'a CommandMru {
    type Item = &'a CommandId;
    type IntoIter = std::collections::vec_deque::Iter<'a, CommandId>;

    fn into_iter(self) -> Self::IntoIter {
        self.recent.iter()
    }
}
