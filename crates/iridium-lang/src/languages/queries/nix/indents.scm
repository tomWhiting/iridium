; From https://github.com/zed-extensions/nix/blob/429479c09c5a597f31b8b53c3950cf96cec6e1f4/languages/nix/indents.scm (MIT)
(formals
  "}" @end) @indent

(indented_string_expression
  "''" @end) @indent

(parenthesized_expression
  ")" @end) @indent

(attrset_expression
  "}" @end) @indent

(let_attrset_expression
  "}" @end) @indent

(rec_attrset_expression
  "}" @end) @indent

(list_expression
  "]" @end) @indent
