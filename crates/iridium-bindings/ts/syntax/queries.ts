/**
 * Tree-sitter highlight queries for each language.
 *
 * Note: These queries are simplified to ensure compatibility with web-tree-sitter.
 * Only basic node patterns are used - no alternation syntax or predicates.
 */

export const HIGHLIGHT_QUERIES: Record<string, string> = {
  rust: `(line_comment) @comment
(block_comment) @comment
(string_literal) @string
(integer_literal) @number
(identifier) @variable`,

  typescript: `
(comment) @comment
(string) @string
(template_string) @string
(number) @number
(true) @boolean
(false) @boolean
(null) @constant.builtin
(undefined) @constant.builtin
(type_identifier) @type
(predefined_type) @type.builtin
(function_declaration name: (identifier) @function)
(identifier) @variable
(property_identifier) @property
`,

  tsx: `
(comment) @comment
(string) @string
(template_string) @string
(number) @number
(true) @boolean
(false) @boolean
(null) @constant.builtin
(undefined) @constant.builtin
(type_identifier) @type
(jsx_opening_element name: (identifier) @tag)
(jsx_closing_element name: (identifier) @tag)
(jsx_self_closing_element name: (identifier) @tag)
(function_declaration name: (identifier) @function)
(identifier) @variable
(property_identifier) @property
`,

  javascript: `
(comment) @comment
(string) @string
(template_string) @string
(number) @number
(true) @boolean
(false) @boolean
(null) @constant.builtin
(undefined) @constant.builtin
(function_declaration name: (identifier) @function)
(identifier) @variable
(property_identifier) @property
`,

  python: `
(comment) @comment
(string) @string
(concatenated_string) @string
(integer) @number
(float) @number
(true) @boolean
(false) @boolean
(none) @constant.builtin
(function_definition name: (identifier) @function)
(class_definition name: (identifier) @type)
(identifier) @variable
`,

  go: `
(comment) @comment
(interpreted_string_literal) @string
(raw_string_literal) @string
(rune_literal) @string
(int_literal) @number
(float_literal) @number
(true) @boolean
(false) @boolean
(nil) @constant.builtin
(type_identifier) @type
(function_declaration name: (identifier) @function)
(identifier) @variable
(field_identifier) @property
`,

  json: `
(string) @string
(number) @number
(true) @boolean
(false) @boolean
(null) @constant.builtin
(pair key: (string) @property)
`,

  yaml: `
(string_scalar) @string
(double_quote_scalar) @string
(single_quote_scalar) @string
(integer_scalar) @number
(float_scalar) @number
(boolean_scalar) @boolean
(null_scalar) @constant.builtin
(comment) @comment
`,

  css: `
(tag_name) @tag
(class_name) @type
(id_name) @property
(property_name) @property
(string_value) @string
(integer_value) @number
(float_value) @number
(comment) @comment
`,

  html: `
(tag_name) @tag
(attribute_name) @property
(attribute_value) @string
(text) @text
(comment) @comment
`,

  cpp: `
(comment) @comment
(string_literal) @string
(raw_string_literal) @string
(char_literal) @string
(number_literal) @number
(true) @boolean
(false) @boolean
(null) @constant.builtin
(type_identifier) @type
(primitive_type) @type.builtin
(identifier) @variable
(field_identifier) @property
`,

  toml: `
(bare_key) @property
(quoted_key) @property
(string) @string
(integer) @number
(float) @number
(boolean) @boolean
(comment) @comment
`,

  lua: `
(comment) @comment
(string) @string
(number) @number
(true) @boolean
(false) @boolean
(nil) @constant.builtin
(function_declaration name: (identifier) @function)
(identifier) @variable
`,
};
