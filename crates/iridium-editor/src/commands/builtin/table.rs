//! The built-in command table.
//!
//! Split from [`super`], which owns the registration built on top of it,
//! because this file is *data*: one [`CommandMeta`] per command, in the
//! declaration order that
//! [`CommandRegistry::commands`](crate::commands::CommandRegistry::commands)
//! enumerates. See the module docs of [`super`] for what declaring a command
//! costs and for why the entries carry aliases.
//!
//! The ids are imported by name rather than by glob, which the workspace's
//! `wildcard_imports` lint rules out; the list also means an entry removed
//! without its import leaves an unused import behind rather than nothing.

use crate::commands::{CommandCategory, CommandMeta};

use super::ids::{
    AST_CURSOR_NODE_END, AST_CURSOR_NODE_START, AST_CURSOR_ON_EVERY_CHILD,
    AST_CURSOR_ON_EVERY_SIBLING, AST_EXPAND_SELECTION, AST_EXTEND_NEXT_SIBLING,
    AST_EXTEND_PREVIOUS_SIBLING, AST_NEXT_CLASS, AST_NEXT_FUNCTION, AST_PREVIOUS_CLASS,
    AST_PREVIOUS_FUNCTION, AST_SELECT_CLASS_AROUND, AST_SELECT_CLASS_INSIDE,
    AST_SELECT_COMMENT_AROUND, AST_SELECT_FIRST_CHILD, AST_SELECT_FUNCTION_AROUND,
    AST_SELECT_FUNCTION_INSIDE, AST_SELECT_LAST_CHILD, AST_SELECT_NEXT_SIBLING, AST_SELECT_NODE,
    AST_SELECT_PREVIOUS_SIBLING, AST_SHRINK_SELECTION, CLIPBOARD_COPY, CLIPBOARD_CUT,
    CLIPBOARD_PASTE, COMMAND_NO_OP, COMMENT_TOGGLE_BLOCK, COMMENT_TOGGLE_LINE, CURSOR_CHAR_LEFT,
    CURSOR_CHAR_LEFT_SELECT, CURSOR_CHAR_RIGHT, CURSOR_CHAR_RIGHT_SELECT, CURSOR_DOCUMENT_END,
    CURSOR_DOCUMENT_END_SELECT, CURSOR_DOCUMENT_START, CURSOR_DOCUMENT_START_SELECT,
    CURSOR_LINE_DOWN, CURSOR_LINE_DOWN_SELECT, CURSOR_LINE_END, CURSOR_LINE_END_SELECT,
    CURSOR_LINE_START, CURSOR_LINE_START_SELECT, CURSOR_LINE_UP, CURSOR_LINE_UP_SELECT,
    CURSOR_WORD_LEFT, CURSOR_WORD_LEFT_SELECT, CURSOR_WORD_RIGHT, CURSOR_WORD_RIGHT_SELECT,
    EDIT_DELETE_BACKWARD, EDIT_DELETE_FORWARD, EDIT_DELETE_TO_LINE_END, EDIT_DELETE_TO_LINE_START,
    EDIT_DELETE_WORD_BACKWARD, EDIT_DELETE_WORD_FORWARD, EDIT_INSERT_CHARACTER,
    EDIT_INSERT_NEWLINE, EDIT_OUTDENT, EDIT_TAB, HISTORY_NEXT_BRANCH, HISTORY_PREVIOUS_BRANCH,
    HISTORY_REDO, HISTORY_REDO_BRANCH, HISTORY_UNDO, LINES_DELETE, LINES_DUPLICATE_DOWN,
    LINES_DUPLICATE_UP, LINES_JOIN, LINES_MOVE_DOWN, LINES_MOVE_UP, MULTI_CURSOR_ADD_CURSOR_ABOVE,
    MULTI_CURSOR_ADD_CURSOR_BELOW, MULTI_CURSOR_ADD_SELECTION_TO_NEXT_MATCH,
    MULTI_CURSOR_REMOVE_LAST_CURSOR, MULTI_CURSOR_SELECT_ALL_OCCURRENCES,
    MULTI_CURSOR_SKIP_LAST_OCCURRENCE, SEARCH_NEXT_MATCH, SEARCH_OPEN, SEARCH_PREVIOUS_MATCH,
    SELECTION_COLLAPSE_TO_PRIMARY, SELECTION_SELECT_ALL, TRANSFORM_CAMEL_CASE,
    TRANSFORM_DEDUPE_LINES, TRANSFORM_KEBAB_CASE, TRANSFORM_LOWER_CASE, TRANSFORM_PASCAL_CASE,
    TRANSFORM_REVERSE_LINES, TRANSFORM_SCREAMING_SNAKE_CASE, TRANSFORM_SNAKE_CASE,
    TRANSFORM_SORT_LINES, TRANSFORM_SORT_LINES_REVERSE, TRANSFORM_SWAP_CASE, TRANSFORM_TITLE_CASE,
    TRANSFORM_TOGGLE_CASE, TRANSFORM_TRIM_TRAILING_WHITESPACE, TRANSFORM_UPPER_CASE,
};

const NAV: CommandCategory = CommandCategory::NAVIGATION;
const SEL: CommandCategory = CommandCategory::SELECTION;
const EDIT: CommandCategory = CommandCategory::EDITING;
const LINES: CommandCategory = CommandCategory::LINES;
const TRANSFORM: CommandCategory = CommandCategory::TRANSFORM;
const COMMENTS: CommandCategory = CommandCategory::COMMENTS;
const CLIP: CommandCategory = CommandCategory::CLIPBOARD;
const HISTORY: CommandCategory = CommandCategory::HISTORY;
const MULTI: CommandCategory = CommandCategory::MULTI_CURSOR;
const SYNTAX: CommandCategory = CommandCategory::SYNTAX;
const SEARCH: CommandCategory = CommandCategory::SEARCH;
const GENERAL: CommandCategory = CommandCategory::GENERAL;

/// Every built-in command's metadata, in declaration order.
///
/// `static` so it costs nothing to enumerate and so the count below cannot drift.
pub static BUILTIN: &[CommandMeta] = &[
    // ----- Navigation -----
    CommandMeta::from_static(CURSOR_CHAR_LEFT, "Cursor Left", NAV),
    CommandMeta::from_static(CURSOR_CHAR_RIGHT, "Cursor Right", NAV),
    CommandMeta::from_static(CURSOR_WORD_LEFT, "Cursor Word Left", NAV),
    CommandMeta::from_static(CURSOR_WORD_RIGHT, "Cursor Word Right", NAV),
    CommandMeta::described(
        CURSOR_LINE_UP,
        "Cursor Up",
        "Keeps each cursor's sticky preferred column.",
        NAV,
    ),
    CommandMeta::described(
        CURSOR_LINE_DOWN,
        "Cursor Down",
        "Keeps each cursor's sticky preferred column.",
        NAV,
    ),
    CommandMeta::described(
        CURSOR_LINE_START,
        "Cursor to Line Start",
        "Smart home: the first non-whitespace character, or column zero when already there.",
        NAV,
    )
    .with_aliases(&["bol", "home"]),
    CommandMeta::from_static(CURSOR_LINE_END, "Cursor to Line End", NAV).with_aliases(&["eol"]),
    CommandMeta::described(
        CURSOR_DOCUMENT_START,
        "Cursor to Document Start",
        "Merges every cursor into one.",
        NAV,
    )
    .with_aliases(&["bof", "top"]),
    CommandMeta::described(
        CURSOR_DOCUMENT_END,
        "Cursor to Document End",
        "Merges every cursor into one.",
        NAV,
    )
    .with_aliases(&["eof", "bottom"]),
    // ----- Selection -----
    CommandMeta::from_static(CURSOR_CHAR_LEFT_SELECT, "Extend Selection Left", SEL),
    CommandMeta::from_static(CURSOR_CHAR_RIGHT_SELECT, "Extend Selection Right", SEL),
    CommandMeta::from_static(CURSOR_WORD_LEFT_SELECT, "Extend Selection Word Left", SEL),
    CommandMeta::from_static(CURSOR_WORD_RIGHT_SELECT, "Extend Selection Word Right", SEL),
    CommandMeta::from_static(CURSOR_LINE_UP_SELECT, "Extend Selection Up", SEL),
    CommandMeta::from_static(CURSOR_LINE_DOWN_SELECT, "Extend Selection Down", SEL),
    CommandMeta::from_static(
        CURSOR_LINE_START_SELECT,
        "Extend Selection to Line Start",
        SEL,
    ),
    CommandMeta::from_static(CURSOR_LINE_END_SELECT, "Extend Selection to Line End", SEL),
    CommandMeta::from_static(
        CURSOR_DOCUMENT_START_SELECT,
        "Extend Selection to Document Start",
        SEL,
    ),
    CommandMeta::from_static(
        CURSOR_DOCUMENT_END_SELECT,
        "Extend Selection to Document End",
        SEL,
    ),
    CommandMeta::from_static(SELECTION_SELECT_ALL, "Select All", SEL),
    CommandMeta::described(
        SELECTION_COLLAPSE_TO_PRIMARY,
        "Collapse to Single Cursor",
        "Drops every secondary cursor and collapses the primary selection.",
        SEL,
    )
    .with_aliases(&["one cursor", "exit multi cursor"]),
    // ----- Editing -----
    CommandMeta::described(
        EDIT_INSERT_CHARACTER,
        "Insert Character",
        "The typing fall-through. Not bound to any key sequence: it runs when a keypress \
             matches no binding and carries text.",
        EDIT,
    )
    .mutating(),
    CommandMeta::described(
        EDIT_INSERT_NEWLINE,
        "Insert Line Break",
        "Applies auto-indent, bracket-block expansion and code-fence expansion when enabled.",
        EDIT,
    )
    .mutating()
    .with_aliases(&["newline", "enter", "return"]),
    CommandMeta::described(
        EDIT_TAB,
        "Tab",
        "Indents every touched line when anything is selected; otherwise inserts a tab or \
             pads to the next tab stop.",
        EDIT,
    )
    .mutating()
    .with_aliases(&["indent"]),
    CommandMeta::from_static(EDIT_OUTDENT, "Outdent", EDIT)
        .mutating()
        .with_aliases(&["unindent", "dedent"]),
    CommandMeta::described(
        EDIT_DELETE_BACKWARD,
        "Delete Backward",
        "Deletes the selection, or the character before each caret.",
        EDIT,
    )
    .mutating()
    .with_aliases(&["backspace", "erase"]),
    CommandMeta::from_static(EDIT_DELETE_WORD_BACKWARD, "Delete Word Backward", EDIT).mutating(),
    CommandMeta::described(
        EDIT_DELETE_FORWARD,
        "Delete Forward",
        "Deletes the selection, or the character after each caret.",
        EDIT,
    )
    .mutating()
    .with_aliases(&["del", "erase"]),
    CommandMeta::from_static(EDIT_DELETE_WORD_FORWARD, "Delete Word Forward", EDIT).mutating(),
    CommandMeta::described(
        EDIT_DELETE_TO_LINE_START,
        "Delete to Line Start",
        "Deletes the selection, or from each caret back to the start of its line.",
        EDIT,
    )
    .mutating()
    .with_aliases(&["kill line backward", "erase"]),
    CommandMeta::described(
        EDIT_DELETE_TO_LINE_END,
        "Delete to Line End",
        "Deletes the selection, or from each caret to the end of its line.",
        EDIT,
    )
    .mutating()
    .with_aliases(&["kill line", "erase"]),
    // ----- Transformations -----
    //
    // Aliases matter more here than anywhere else in the table: nobody
    // remembers whether this editor calls it "screaming snake" or "constant
    // case", and a palette that only matches the name it happens to use is a
    // palette you have to already know.
    CommandMeta::described(
        TRANSFORM_UPPER_CASE,
        "Upper Case",
        "Uppercases each caret's selection, or the word under it.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["uppercase", "ucase", "caps", "shout"]),
    CommandMeta::described(
        TRANSFORM_LOWER_CASE,
        "Lower Case",
        "Lowercases each caret's selection, or the word under it.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["lowercase", "lcase", "downcase"]),
    CommandMeta::described(
        TRANSFORM_TITLE_CASE,
        "Title Case",
        "Capitalises each word, keeping the original spacing and punctuation.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["titlecase", "capitalize", "capitalise"]),
    CommandMeta::described(
        TRANSFORM_TOGGLE_CASE,
        "Toggle Case",
        "Cycles the text through lower, upper and title case.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["cycle case"]),
    CommandMeta::described(
        TRANSFORM_SWAP_CASE,
        "Swap Case",
        "Inverts the case of every cased character.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["invert case", "flip case"]),
    CommandMeta::described(
        TRANSFORM_CAMEL_CASE,
        "camelCase",
        "Re-joins the words as camelCase.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["camel", "lower camel", "mixed case"]),
    CommandMeta::described(
        TRANSFORM_PASCAL_CASE,
        "PascalCase",
        "Re-joins the words as PascalCase.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["pascal", "upper camel", "studly"]),
    CommandMeta::described(
        TRANSFORM_SNAKE_CASE,
        "snake_case",
        "Re-joins the words as snake_case.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["snake", "underscore"]),
    CommandMeta::described(
        TRANSFORM_SCREAMING_SNAKE_CASE,
        "SCREAMING_SNAKE_CASE",
        "Re-joins the words as SCREAMING_SNAKE_CASE.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&[
        "screaming snake",
        "constant case",
        "macro case",
        "upper snake",
    ]),
    CommandMeta::described(
        TRANSFORM_KEBAB_CASE,
        "kebab-case",
        "Re-joins the words as kebab-case.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["kebab", "dash case", "hyphen case", "lisp case", "slug"]),
    CommandMeta::described(
        TRANSFORM_SORT_LINES,
        "Sort Lines",
        "Sorts the selected lines ascending, by Unicode scalar value.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["sort", "order lines", "alphabetize", "alphabetise"]),
    CommandMeta::described(
        TRANSFORM_SORT_LINES_REVERSE,
        "Sort Lines Descending",
        "Sorts the selected lines descending, by Unicode scalar value.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["sort reverse", "sort desc", "rsort"]),
    CommandMeta::described(
        TRANSFORM_REVERSE_LINES,
        "Reverse Lines",
        "Reverses the order of the selected lines.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["flip lines", "invert lines"]),
    CommandMeta::described(
        TRANSFORM_DEDUPE_LINES,
        "Remove Duplicate Lines",
        "Removes repeated lines, keeping the first of each and the rest of the order.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["dedupe", "dedup", "uniq", "unique", "distinct"]),
    CommandMeta::described(
        TRANSFORM_TRIM_TRAILING_WHITESPACE,
        "Trim Trailing Whitespace",
        "Strips trailing whitespace from the selected lines.",
        TRANSFORM,
    )
    .mutating()
    .with_aliases(&["trim", "strip whitespace", "rtrim"]),
    // ----- Lines -----
    CommandMeta::from_static(LINES_MOVE_UP, "Move Line Up", LINES)
        .mutating()
        .with_aliases(&["swap line up"]),
    CommandMeta::from_static(LINES_MOVE_DOWN, "Move Line Down", LINES)
        .mutating()
        .with_aliases(&["swap line down"]),
    CommandMeta::from_static(LINES_DUPLICATE_UP, "Duplicate Line Up", LINES)
        .mutating()
        .with_aliases(&["clone", "dupe"]),
    CommandMeta::from_static(LINES_DUPLICATE_DOWN, "Duplicate Line Down", LINES)
        .mutating()
        .with_aliases(&["clone", "dupe"]),
    CommandMeta::from_static(LINES_DELETE, "Delete Line", LINES)
        .mutating()
        .with_aliases(&["kill line", "remove line"]),
    CommandMeta::from_static(LINES_JOIN, "Join Lines", LINES)
        .mutating()
        .with_aliases(&["merge lines"]),
    // ----- Comments -----
    CommandMeta::described(
        COMMENT_TOGGLE_LINE,
        "Toggle Line Comment",
        "Uses the document language's comment syntax, falling back to the configured token; \
             acknowledged without editing when neither exists.",
        COMMENTS,
    )
    .mutating()
    .with_aliases(&["uncomment", "//"]),
    CommandMeta::described(
        COMMENT_TOGGLE_BLOCK,
        "Toggle Block Comment",
        "Falls back to the line comment toggle for languages with no block pair.",
        COMMENTS,
    )
    .mutating()
    .with_aliases(&["uncomment", "/*"]),
    // ----- Clipboard -----
    CommandMeta::described(
        CLIPBOARD_COPY,
        "Copy",
        "Copies the selections, or each cursor's whole line when nothing is selected.",
        CLIP,
    )
    .with_aliases(&["yank"]),
    CommandMeta::described(
        CLIPBOARD_CUT,
        "Cut",
        "Always updates the clipboard, even when nothing can be removed.",
        CLIP,
    )
    .mutating()
    .with_aliases(&["kill"]),
    CommandMeta::described(
        CLIPBOARD_PASTE,
        "Paste",
        "Requests clipboard text from the host, then inserts it at every cursor.",
        CLIP,
    )
    .mutating()
    .with_aliases(&["put"]),
    // ----- History -----
    CommandMeta::described(
        HISTORY_UNDO,
        "Undo",
        "Steps back along the active branch of the undo tree.",
        HISTORY,
    )
    .mutating()
    .with_aliases(&["revert"]),
    CommandMeta::described(
        HISTORY_REDO,
        "Redo",
        "Steps forward along the active branch of the undo tree.",
        HISTORY,
    )
    .mutating()
    .with_aliases(&["reapply"]),
    CommandMeta::described(
        HISTORY_REDO_BRANCH,
        "Redo Into Branch",
        "Steps forward into the branch named by the count, making it the active path.",
        HISTORY,
    )
    .mutating()
    .with_aliases(&["redo branch", "enter branch"]),
    CommandMeta::described(
        HISTORY_NEXT_BRANCH,
        "Next Undo Branch",
        "Points redo at the next branch of the current node. Changes nothing in the document.",
        HISTORY,
    )
    .with_aliases(&["next branch", "cycle branch", "other future"]),
    CommandMeta::described(
        HISTORY_PREVIOUS_BRANCH,
        "Previous Undo Branch",
        "Points redo at the previous branch of the current node. Changes nothing in the document.",
        HISTORY,
    )
    .with_aliases(&["previous branch", "prev branch"]),
    // ----- Multi-cursor -----
    CommandMeta::described(
        MULTI_CURSOR_ADD_SELECTION_TO_NEXT_MATCH,
        "Add Selection to Next Find Match",
        "Selects the word at the caret first when nothing is selected.",
        MULTI,
    )
    .with_aliases(&["next occurrence"]),
    CommandMeta::from_static(
        MULTI_CURSOR_SELECT_ALL_OCCURRENCES,
        "Select All Occurrences",
        MULTI,
    ),
    CommandMeta::described(
        MULTI_CURSOR_REMOVE_LAST_CURSOR,
        "Remove Last Cursor",
        "Pops the most recently added cursor; a no-op once an edit or motion has invalidated \
             the addition order.",
        MULTI,
    ),
    CommandMeta::from_static(MULTI_CURSOR_ADD_CURSOR_ABOVE, "Add Cursor Above", MULTI),
    CommandMeta::from_static(MULTI_CURSOR_ADD_CURSOR_BELOW, "Add Cursor Below", MULTI),
    CommandMeta::described(
        MULTI_CURSOR_SKIP_LAST_OCCURRENCE,
        "Move Last Selection to Next Find Match",
        "Drops the most recently added occurrence cursor and takes the next one.",
        MULTI,
    )
    .with_aliases(&["skip occurrence"]),
    // ----- Syntax -----
    //
    // Not `.mutating()`: every one of these produces a selection change and
    // nothing else, so they stay available in a read-only buffer — which is
    // where reading structure out of a file matters most.
    CommandMeta::described(
        AST_SELECT_NODE,
        "Select Syntax Node",
        "Snaps each selection to the smallest syntax node that covers it.",
        SYNTAX,
    )
    .with_aliases(&["select node", "snap to node", "select element"]),
    CommandMeta::described(
        AST_EXPAND_SELECTION,
        "Expand Selection",
        "Widens each selection to the smallest syntax node that strictly contains it.",
        SYNTAX,
    )
    .with_aliases(&[
        "select larger syntax node",
        "grow selection",
        "widen selection",
        "select parent",
        "expand region",
    ]),
    CommandMeta::described(
        AST_SHRINK_SELECTION,
        "Shrink Selection",
        "Undoes one expansion, restoring the selections exactly as they were.",
        SYNTAX,
    )
    .with_aliases(&[
        "select smaller syntax node",
        "narrow selection",
        "select child",
        "contract selection",
    ]),
    CommandMeta::described(
        AST_SELECT_NEXT_SIBLING,
        "Select Next Sibling Node",
        "Selects the next syntax node beside the current one, climbing to the \
         enclosing node when there is none.",
        SYNTAX,
    )
    .with_aliases(&["next sibling", "next node", "select next element"]),
    CommandMeta::described(
        AST_SELECT_PREVIOUS_SIBLING,
        "Select Previous Sibling Node",
        "Selects the previous syntax node beside the current one, climbing to \
         the enclosing node when there is none.",
        SYNTAX,
    )
    .with_aliases(&[
        "previous sibling",
        "previous node",
        "select previous element",
    ]),
    CommandMeta::described(
        AST_SELECT_FIRST_CHILD,
        "Select First Child Node",
        "Selects the first child of the syntax node under each selection.",
        SYNTAX,
    )
    .with_aliases(&["first child", "descend", "select inner"]),
    CommandMeta::described(
        AST_SELECT_LAST_CHILD,
        "Select Last Child Node",
        "Selects the last child of the syntax node under each selection.",
        SYNTAX,
    )
    .with_aliases(&["last child"]),
    CommandMeta::described(
        AST_EXTEND_NEXT_SIBLING,
        "Extend Selection To Next Sibling",
        "Grows each selection to also cover the syntax node after it, including \
         the punctuation between them.",
        SYNTAX,
    )
    .with_aliases(&["extend next", "add next node", "grow to next sibling"]),
    CommandMeta::described(
        AST_EXTEND_PREVIOUS_SIBLING,
        "Extend Selection To Previous Sibling",
        "Grows each selection to also cover the syntax node before it, including \
         the punctuation between them.",
        SYNTAX,
    )
    .with_aliases(&["extend previous", "add previous node"]),
    CommandMeta::described(
        AST_CURSOR_NODE_START,
        "Go To Node Start",
        "Moves each caret to the start of the syntax node it sits in. Pressing \
         again walks outward to the enclosing node.",
        SYNTAX,
    )
    .with_aliases(&["node start", "beginning of node", "jump to node start"]),
    CommandMeta::described(
        AST_CURSOR_NODE_END,
        "Go To Node End",
        "Moves each caret to the end of the syntax node it sits in. Pressing \
         again walks outward to the enclosing node.",
        SYNTAX,
    )
    .with_aliases(&["node end", "end of node", "jump to node end"]),
    CommandMeta::described(
        AST_CURSOR_ON_EVERY_SIBLING,
        "Cursor On Every Sibling Node",
        "Puts a cursor on every syntax node beside the current one, including \
         it — one per element of the array or field of the object.",
        SYNTAX,
    )
    .with_aliases(&[
        "cursor per sibling",
        "select all siblings",
        "multi cursor siblings",
    ]),
    CommandMeta::described(
        AST_CURSOR_ON_EVERY_CHILD,
        "Cursor On Every Child Node",
        "Puts a cursor on every child of the syntax node under each selection.",
        SYNTAX,
    )
    .with_aliases(&[
        "cursor per child",
        "select all children",
        "multi cursor children",
    ]),
    CommandMeta::described(
        AST_SELECT_FUNCTION_INSIDE,
        "Select Inside Function",
        "Selects the body of the function under each selection, leaving the \
         signature behind. Pressing again walks out to the function holding it.",
        SYNTAX,
    )
    .with_aliases(&["inner function", "function body", "select function body"]),
    CommandMeta::described(
        AST_SELECT_FUNCTION_AROUND,
        "Select Function",
        "Selects the whole function under each selection, signature and body \
         together. Pressing again walks out to the function holding it.",
        SYNTAX,
    )
    .with_aliases(&["around function", "outer function", "select whole function"]),
    CommandMeta::described(
        AST_SELECT_CLASS_INSIDE,
        "Select Inside Class",
        "Selects the body of the class, struct, enum or interface under each \
         selection, leaving the declaration behind.",
        SYNTAX,
    )
    .with_aliases(&["inner class", "class body", "select struct body"]),
    CommandMeta::described(
        AST_SELECT_CLASS_AROUND,
        "Select Class",
        "Selects the whole class, struct, enum or interface under each \
         selection — or, in Markdown, the whole section.",
        SYNTAX,
    )
    .with_aliases(&[
        "around class",
        "outer class",
        "select whole class",
        "select struct",
        "select section",
    ]),
    CommandMeta::described(
        AST_SELECT_COMMENT_AROUND,
        "Select Comment",
        "Selects the whole comment under each selection, including every line \
         of a run of line comments. There is no inside variant: no vendored \
         query defines one.",
        SYNTAX,
    )
    .with_aliases(&["around comment", "select whole comment"]),
    CommandMeta::described(
        AST_NEXT_FUNCTION,
        "Go To Next Function",
        "Moves each caret to the start of the next function.",
        SYNTAX,
    )
    .with_aliases(&["next function", "next method", "jump to next function"]),
    CommandMeta::described(
        AST_PREVIOUS_FUNCTION,
        "Go To Previous Function",
        "Moves each caret to the start of the previous function.",
        SYNTAX,
    )
    .with_aliases(&[
        "previous function",
        "previous method",
        "jump to previous function",
    ]),
    CommandMeta::described(
        AST_NEXT_CLASS,
        "Go To Next Class",
        "Moves each caret to the start of the next class, struct, enum or \
         interface — or, in Markdown, the next section heading.",
        SYNTAX,
    )
    .with_aliases(&["next class", "next struct", "next section", "next heading"]),
    CommandMeta::described(
        AST_PREVIOUS_CLASS,
        "Go To Previous Class",
        "Moves each caret to the start of the previous class, struct, enum or \
         interface — or, in Markdown, the previous section heading.",
        SYNTAX,
    )
    .with_aliases(&[
        "previous class",
        "previous struct",
        "previous section",
        "previous heading",
    ]),
    // ----- Search -----
    CommandMeta::from_static(SEARCH_OPEN, "Find", SEARCH).with_aliases(&["search"]),
    CommandMeta::from_static(SEARCH_NEXT_MATCH, "Find Next", SEARCH).with_aliases(&["search next"]),
    CommandMeta::from_static(SEARCH_PREVIOUS_MATCH, "Find Previous", SEARCH)
        .with_aliases(&["search previous"]),
    // ----- General -----
    CommandMeta::described(
        COMMAND_NO_OP,
        "Do Nothing",
        "Swallows the keypress. Bound by a modal keymap so an unhandled key in a \
         non-typing mode is consumed instead of inserted.",
        GENERAL,
    ),
];

/// The number of commands the kernel declares.
///
/// Derived from [`BUILTIN`], so it cannot disagree with the table.
pub const BUILTIN_COMMAND_COUNT: usize = BUILTIN.len();
