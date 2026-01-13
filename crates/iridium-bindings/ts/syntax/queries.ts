/**
 * Tree-sitter highlight queries for each language.
 */

export const HIGHLIGHT_QUERIES: Record<string, string> = {
  rust: `
(identifier) @variable
(type_identifier) @type
(primitive_type) @type.builtin
(self) @variable.special
(field_identifier) @property

(call_expression function: (identifier) @function)
(call_expression function: (scoped_identifier name: (identifier) @function))
(call_expression function: (field_expression field: (field_identifier) @function.method))
(function_item name: (identifier) @function.definition)
(macro_invocation macro: (identifier) @function.special)
(macro_invocation macro: (scoped_identifier name: (identifier) @function.special))

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["." ";" "," "::"] @punctuation.delimiter

["as" "async" "const" "default" "dyn" "enum" "extern" "fn" "impl" "let" "mod" "move" "pub" "ref" "static" "struct" "for" "trait" "type" "union" "unsafe" "use" "where"] @keyword
["await" "break" "continue" "else" "if" "in" "loop" "match" "return" "while" "yield"] @keyword.control
(crate) @keyword
(mutable_specifier) @keyword
(super) @keyword

[(string_literal) (raw_string_literal) (char_literal)] @string
(escape_sequence) @string.escape
[(integer_literal) (float_literal)] @number
(boolean_literal) @boolean
[(line_comment) (block_comment)] @comment
`,

  typescript: `
(identifier) @variable
(property_identifier) @property
(type_identifier) @type
(predefined_type) @type.builtin

(call_expression function: (identifier) @function)
(call_expression function: (member_expression property: (property_identifier) @function.method))
(function_declaration name: (identifier) @function.definition)
(method_definition name: (property_identifier) @function.definition)

["(" ")" "{" "}" "[" "]" "<" ">"] @punctuation.bracket
["." ";" "," ":"] @punctuation.delimiter

["async" "await" "break" "case" "catch" "class" "const" "continue" "debugger" "default" "delete" "do" "else" "export" "extends" "finally" "for" "from" "function" "get" "if" "import" "in" "instanceof" "let" "new" "of" "return" "set" "static" "switch" "throw" "try" "typeof" "var" "void" "while" "with" "yield" "interface" "type" "as" "is" "implements" "namespace" "enum" "abstract" "declare" "private" "protected" "public" "readonly"] @keyword

[(string) (template_string)] @string
(escape_sequence) @string.escape
(number) @number
[(true) (false)] @boolean
(null) @constant.builtin
(undefined) @constant.builtin
(comment) @comment
`,

  tsx: `
(identifier) @variable
(property_identifier) @property
(type_identifier) @type
(predefined_type) @type.builtin

(call_expression function: (identifier) @function)
(call_expression function: (member_expression property: (property_identifier) @function.method))
(function_declaration name: (identifier) @function.definition)
(method_definition name: (property_identifier) @function.definition)

(jsx_opening_element name: (identifier) @tag)
(jsx_closing_element name: (identifier) @tag)
(jsx_self_closing_element name: (identifier) @tag)
(jsx_attribute (property_identifier) @property)

["(" ")" "{" "}" "[" "]" "<" ">"] @punctuation.bracket
["." ";" "," ":"] @punctuation.delimiter

["async" "await" "break" "case" "catch" "class" "const" "continue" "debugger" "default" "delete" "do" "else" "export" "extends" "finally" "for" "from" "function" "get" "if" "import" "in" "instanceof" "let" "new" "of" "return" "set" "static" "switch" "throw" "try" "typeof" "var" "void" "while" "with" "yield" "interface" "type" "as" "is"] @keyword

[(string) (template_string)] @string
(escape_sequence) @string.escape
(number) @number
[(true) (false)] @boolean
(null) @constant.builtin
(undefined) @constant.builtin
(comment) @comment
`,

  javascript: `
(identifier) @variable
(property_identifier) @property

(call_expression function: (identifier) @function)
(call_expression function: (member_expression property: (property_identifier) @function.method))
(function_declaration name: (identifier) @function.definition)
(method_definition name: (property_identifier) @function.definition)

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["." ";" "," ":"] @punctuation.delimiter

["async" "await" "break" "case" "catch" "class" "const" "continue" "debugger" "default" "delete" "do" "else" "export" "extends" "finally" "for" "from" "function" "get" "if" "import" "in" "instanceof" "let" "new" "of" "return" "set" "static" "switch" "throw" "try" "typeof" "var" "void" "while" "with" "yield"] @keyword

[(string) (template_string)] @string
(escape_sequence) @string.escape
(number) @number
[(true) (false)] @boolean
(null) @constant.builtin
(undefined) @constant.builtin
(comment) @comment
`,

  python: `
(identifier) @variable
(attribute attribute: (identifier) @property)
(type (identifier) @type)

(call function: (identifier) @function)
(call function: (attribute attribute: (identifier) @function.method))
(function_definition name: (identifier) @function.definition)
(class_definition name: (identifier) @type.definition)
(decorator) @function.decorator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["." "," ":" ";"] @punctuation.delimiter

["and" "as" "assert" "async" "await" "break" "class" "continue" "def" "del" "elif" "else" "except" "finally" "for" "from" "global" "if" "import" "in" "is" "lambda" "nonlocal" "not" "or" "pass" "raise" "return" "try" "while" "with" "yield" "match" "case"] @keyword

[(string) (concatenated_string)] @string
(escape_sequence) @string.escape
[(integer) (float)] @number
[(true) (false)] @boolean
(none) @constant.builtin
(comment) @comment
`,

  go: `
(identifier) @variable
(field_identifier) @property
(type_identifier) @type

(call_expression function: (identifier) @function)
(call_expression function: (selector_expression field: (field_identifier) @function.method))
(function_declaration name: (identifier) @function.definition)
(method_declaration name: (field_identifier) @function.definition)

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["." "," ";" ":"] @punctuation.delimiter

["break" "case" "chan" "const" "continue" "default" "defer" "else" "fallthrough" "for" "func" "go" "goto" "if" "import" "interface" "map" "package" "range" "return" "select" "struct" "switch" "type" "var"] @keyword

[(interpreted_string_literal) (raw_string_literal) (rune_literal)] @string
(escape_sequence) @string.escape
[(int_literal) (float_literal) (imaginary_literal)] @number
[(true) (false)] @boolean
(nil) @constant.builtin
(comment) @comment
`,

  json: `
(string) @string
(number) @number
[(true) (false)] @boolean
(null) @constant.builtin
(pair key: (string) @property)
["[" "]" "{" "}"] @punctuation.bracket
["," ":"] @punctuation.delimiter
`,

  yaml: `
(string_scalar) @string
(double_quote_scalar) @string
(single_quote_scalar) @string
(integer_scalar) @number
(float_scalar) @number
(boolean_scalar) @boolean
(null_scalar) @constant.builtin
(block_mapping_pair key: (_) @property)
(comment) @comment
`,

  css: `
(tag_name) @tag
(class_name) @type
(id_name) @property
(property_name) @property
(feature_name) @property

(string_value) @string
(color_value) @string.special
(integer_value) @number
(float_value) @number
(plain_value) @constant

["{" "}" "(" ")" "[" "]"] @punctuation.bracket
[":" ";" ","] @punctuation.delimiter

["@media" "@import" "@charset" "@namespace" "@keyframes" "@supports" "@font-face" "@page"] @keyword
(important) @keyword
(comment) @comment
`,

  html: `
(tag_name) @tag
(attribute_name) @property
(attribute_value) @string
(text) @text
(comment) @comment

["<" ">" "</" "/>" "="] @punctuation.delimiter
`,

  cpp: `
(identifier) @variable
(field_identifier) @property
(type_identifier) @type
(primitive_type) @type.builtin

(call_expression function: (identifier) @function)
(call_expression function: (field_expression field: (field_identifier) @function.method))
(function_definition declarator: (function_declarator declarator: (identifier) @function.definition))
(preproc_include) @keyword.directive
(preproc_def) @keyword.directive
(preproc_ifdef) @keyword.directive
(preproc_ifndef) @keyword.directive
(preproc_if) @keyword.directive
(preproc_else) @keyword.directive
(preproc_endif) @keyword.directive

["(" ")" "{" "}" "[" "]" "<" ">"] @punctuation.bracket
["." ";" "," "::" "->"] @punctuation.delimiter

["alignas" "alignof" "asm" "auto" "break" "case" "catch" "class" "const" "constexpr" "continue" "default" "delete" "do" "else" "enum" "explicit" "extern" "for" "friend" "goto" "if" "inline" "mutable" "namespace" "new" "noexcept" "operator" "override" "private" "protected" "public" "return" "sizeof" "static" "struct" "switch" "template" "this" "throw" "try" "typedef" "typeid" "typename" "union" "using" "virtual" "volatile" "while"] @keyword

[(string_literal) (raw_string_literal) (char_literal)] @string
(escape_sequence) @string.escape
(number_literal) @number
[(true) (false)] @boolean
(null) @constant.builtin
(comment) @comment
`,

  toml: `
(bare_key) @property
(quoted_key) @property
(string) @string
(integer) @number
(float) @number
(boolean) @boolean
(comment) @comment
["[" "]" "{" "}"] @punctuation.bracket
["." "=" ","] @punctuation.delimiter
`,

  lua: `
(identifier) @variable
(dot_index_expression field: (identifier) @property)
(bracket_index_expression field: (identifier) @property)

(function_call name: (identifier) @function)
(function_call name: (dot_index_expression field: (identifier) @function.method))
(function_declaration name: (identifier) @function.definition)

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["." "," ";" ":"] @punctuation.delimiter

["and" "break" "do" "else" "elseif" "end" "for" "function" "goto" "if" "in" "local" "not" "or" "repeat" "return" "then" "until" "while"] @keyword

(string) @string
(number) @number
[(true) (false)] @boolean
(nil) @constant.builtin
(comment) @comment
`,
};
