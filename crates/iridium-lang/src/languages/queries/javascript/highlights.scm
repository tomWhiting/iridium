; JavaScript Highlights Query
; Compatible with tree-sitter-javascript 0.23.1

; Comments
(comment) @comment
(hash_bang_line) @comment

; Strings
[
  (string)
  (template_string)
] @string

(escape_sequence) @string.escape
(regex) @string.regex
(regex_flags) @keyword

; Numbers
(number) @number

; Boolean
[
  (true)
  (false)
] @boolean

; Null/undefined
[
  (null)
  (undefined)
] @constant.builtin

; Variables
(identifier) @variable

; Properties
(property_identifier) @property
(shorthand_property_identifier) @property
(private_property_identifier) @property

; Function calls
(call_expression
  function: (identifier) @function)

(call_expression
  function: (member_expression
    property: (property_identifier) @function.method))

; Function definitions
(function_expression
  name: (identifier) @function)

(function_declaration
  name: (identifier) @function)

(method_definition
  name: (property_identifier) @function.method)

(arrow_function
  parameter: (identifier) @variable.parameter)

(variable_declarator
  name: (identifier) @function
  value: [(function_expression) (arrow_function)])

; Class definitions
(class_declaration
  name: (identifier) @type)

(class_body
  (method_definition
    name: (property_identifier) @function.method))

; Special identifiers
(this) @variable.special
(super) @variable.special

; Constants (UPPER_CASE identifiers)
((identifier) @constant
  (#match? @constant "^[A-Z_][A-Z\\d_]*$"))

; Keywords
[
  "async"
  "await"
  "class"
  "const"
  "debugger"
  "default"
  "delete"
  "export"
  "extends"
  "from"
  "function"
  "get"
  "import"
  "in"
  "instanceof"
  "let"
  "new"
  "of"
  "set"
  "static"
  "typeof"
  "var"
  "void"
  "with"
] @keyword

; Control flow keywords
[
  "break"
  "case"
  "catch"
  "continue"
  "do"
  "else"
  "finally"
  "for"
  "if"
  "return"
  "switch"
  "throw"
  "try"
  "while"
  "yield"
] @keyword.control

; Operators
[
  "-"
  "--"
  "-="
  "+"
  "++"
  "+="
  "*"
  "*="
  "**"
  "**="
  "/"
  "/="
  "%"
  "%="
  "<"
  "<="
  "<<"
  "<<="
  "="
  "=="
  "==="
  "!"
  "!="
  "!=="
  "=>"
  ">"
  ">="
  ">>"
  ">>="
  ">>>"
  ">>>="
  "~"
  "^"
  "&"
  "|"
  "^="
  "&="
  "|="
  "&&"
  "||"
  "??"
  "&&="
  "||="
  "??="
  "..."
] @operator

; Punctuation
[
  ";"
  "."
  ","
  ":"
] @punctuation.delimiter

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

(ternary_expression
  ["?" ":"] @operator)

(template_substitution
  "${" @punctuation.special
  "}" @punctuation.special) @embedded

; JSX (if available in grammar)
(jsx_opening_element
  name: (identifier) @tag)

(jsx_closing_element
  name: (identifier) @tag)

(jsx_self_closing_element
  name: (identifier) @tag)

(jsx_attribute
  (property_identifier) @attribute)
