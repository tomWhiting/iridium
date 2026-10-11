; Iridium TOML highlights for tree-sitter-toml-ng 0.7.0.
(bare_key) @property
[(quoted_key) (string)] @string
(comment) @comment
(boolean) @boolean
[(integer) (float)] @number
[(offset_date_time) (local_date_time) (local_date) (local_time)] @string
"=" @operator
["[" "]" "[[" "]]" "{" "}"] @punctuation.bracket
["." ","] @punctuation.delimiter
