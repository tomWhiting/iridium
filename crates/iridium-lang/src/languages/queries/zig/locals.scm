; Adapted from nvim-treesitter (https://github.com/nvim-treesitter/nvim-treesitter)
; Licensed under the Apache License, Version 2.0
; Source: queries/zig/locals.scm @ commit cf12346a3414fa1b06af75c79faebe7f76df080a
; Modifications: <see git history>

; Definitions
(function_declaration
  name: (identifier) @local.definition.function)

(parameter
  name: (identifier) @local.definition.parameter)

(variable_declaration
  (identifier) @local.definition.var)

(variable_declaration
  (identifier) @local.definition.type
  (enum_declaration))

(container_field
  type: (identifier) @local.definition.field)

(enum_declaration
  (function_declaration
    name: (identifier) @local.definition.method))

(variable_declaration
  (identifier) @local.definition.type
  (struct_declaration))

(struct_declaration
  (function_declaration
    name: (identifier) @local.definition.method))

(container_field
  name: (identifier) @local.definition.field)

(variable_declaration
  (identifier) @local.definition.type
  (union_declaration))

(union_declaration
  (function_declaration
    name: (identifier) @local.definition.method))

(payload
  (identifier) @local.definition.var)

(block_label
  (identifier) @local.definition)

; References
(identifier) @local.reference

(parameter
  type: (identifier) @local.reference
  (#set! reference.kind "type"))

(pointer_type
  (identifier) @local.reference
  (#set! reference.kind "type"))

(nullable_type
  (identifier) @local.reference
  (#set! reference.kind "type"))

(struct_initializer
  (identifier) @local.reference
  (#set! reference.kind "type"))

(array_type
  (_)
  (identifier) @local.reference
  (#set! reference.kind "type"))

(slice_type
  (identifier) @local.reference
  (#set! reference.kind "type"))

(field_expression
  member: (identifier) @local.reference
  (#set! reference.kind "field"))

(call_expression
  function: (field_expression
    member: (identifier) @local.reference
    (#set! reference.kind "function")))

(break_label
  (identifier) @local.reference)

[
  (for_statement)
  (if_statement)
  (while_statement)
  (function_declaration)
  (block)
  (source_file)
  (enum_declaration)
  (struct_declaration)
] @local.scope
