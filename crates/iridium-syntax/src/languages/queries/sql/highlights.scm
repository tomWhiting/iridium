; SQL Highlights Query
; Minimal highlights.scm for tree-sitter-sql

; Comments
(comment) @comment
(marginalia) @comment

; Strings
(literal) @string

; Numbers
(number) @number

; Keywords
[
  "select"
  "from"
  "where"
  "join"
  "inner"
  "outer"
  "left"
  "right"
  "full"
  "cross"
  "on"
  "and"
  "or"
  "not"
  "in"
  "between"
  "like"
  "is"
  "null"
  "as"
  "order"
  "by"
  "asc"
  "desc"
  "limit"
  "offset"
  "group"
  "having"
  "union"
  "all"
  "distinct"
  "insert"
  "into"
  "values"
  "update"
  "set"
  "delete"
  "create"
  "table"
  "index"
  "view"
  "drop"
  "alter"
  "add"
  "column"
  "primary"
  "key"
  "foreign"
  "references"
  "constraint"
  "unique"
  "check"
  "default"
  "cascade"
  "case"
  "when"
  "then"
  "else"
  "end"
  "exists"
  "with"
  "recursive"
  "true"
  "false"
] @keyword

; Functions
(function_call
  name: (identifier) @function)

; Identifiers and columns
(identifier) @variable
(column_reference) @property
(table_reference) @type

; Operators
[
  "="
  "!="
  "<>"
  "<"
  ">"
  "<="
  ">="
  "+"
  "-"
  "*"
  "/"
  "%"
  "||"
] @operator

; Punctuation
[
  "("
  ")"
] @punctuation.bracket

[
  ","
  ";"
  "."
] @punctuation.delimiter
