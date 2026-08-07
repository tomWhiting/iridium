; C++ Highlights Query
; Compatible with tree-sitter-cpp 0.23.4

; Comments
(comment) @comment

; Strings
[
  (string_literal)
  (system_lib_string)
  (char_literal)
  (raw_string_literal)
] @string

; Numbers
(number_literal) @number

; Boolean
[
  (true)
  (false)
] @boolean

; Null
(null) @constant.builtin

; Identifiers
(identifier) @variable
(field_identifier) @property
(namespace_identifier) @namespace

; Function calls
(call_expression
  function: (identifier) @function)

(call_expression
  function: (qualified_identifier
    name: (identifier) @function))

(call_expression
  function: (field_expression
    field: (field_identifier) @function.method))

; Function definitions
(function_declarator
  declarator: (identifier) @function)

(function_declarator
  declarator: (qualified_identifier
    name: (identifier) @function))

(function_declarator
  declarator: (field_identifier) @function)

(template_function
  name: (identifier) @function)

(template_method
  name: (field_identifier) @function)

(destructor_name (identifier) @function)

(operator_name) @function

; Preprocessor function
(preproc_function_def
  name: (identifier) @function.special)

; Types
(type_identifier) @type
(primitive_type) @type.builtin
(sized_type_specifier) @type.builtin
(auto) @type

((namespace_identifier) @type
  (#match? @type "^[A-Z]"))

; Constants (UPPER_CASE identifiers)
((identifier) @constant.builtin
  (#match? @constant.builtin "^[A-Z_][A-Z\\d_]*$"))

; Labels
(statement_identifier) @label

; Special
(this) @variable.builtin

; Attributes
(attribute
  name: (identifier) @attribute)

; Keywords
[
  "alignas"
  "alignof"
  "class"
  "concept"
  "consteval"
  "constexpr"
  "constinit"
  "decltype"
  "delete"
  "enum"
  "explicit"
  "extern"
  "final"
  "friend"
  "inline"
  "namespace"
  "new"
  "noexcept"
  "operator"
  "override"
  "private"
  "protected"
  "public"
  "requires"
  "sizeof"
  "struct"
  "template"
  "thread_local"
  "typedef"
  "typename"
  "union"
  "using"
  "virtual"
  (storage_class_specifier)
  (type_qualifier)
] @keyword

; Control flow keywords
[
  "break"
  "case"
  "catch"
  "co_await"
  "co_return"
  "co_yield"
  "continue"
  "default"
  "do"
  "else"
  "for"
  "goto"
  "if"
  "return"
  "switch"
  "throw"
  "try"
  "while"
] @keyword.control

; Preprocessor directives
[
  "#define"
  "#elif"
  "#else"
  "#endif"
  "#if"
  "#ifdef"
  "#ifndef"
  "#include"
  (preproc_directive)
] @keyword.directive

; Operators
[
  "."
  ".*"
  "->*"
  "~"
  "-"
  "--"
  "-="
  "->"
  "="
  "!"
  "!="
  "|"
  "|="
  "||"
  "^"
  "^="
  "&"
  "&="
  "&&"
  "+"
  "++"
  "+="
  "*"
  "*="
  "/"
  "/="
  "%"
  "%="
  "<<"
  "<<="
  ">>"
  ">>="
  "<"
  "=="
  ">"
  "<="
  ">="
  "?"
  "<=>"
] @operator

; Punctuation
[
  ","
  ":"
  "::"
  ";"
] @punctuation.delimiter

[
  "{"
  "}"
  "("
  ")"
  "["
  "]"
] @punctuation.bracket
