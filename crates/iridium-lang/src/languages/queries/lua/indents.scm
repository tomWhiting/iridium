; From https://github.com/zed-extensions/lua/blob/ce3fde1f675b32ce2de032e32609aba18cbf8121/languages/lua/indents.scm (Apache-2.0)
(if_statement
  "end" @end) @indent

(do_statement
  "end" @end) @indent

(while_statement
  "end" @end) @indent

(for_statement
  "end" @end) @indent

(repeat_statement
  "until" @end) @indent

(function_declaration
  "end" @end) @indent

(function_definition
  "end" @end) @indent

(_
  "["
  "]" @end) @indent

(_
  "{"
  "}" @end) @indent

(_
  "("
  ")" @end) @indent
