; Adapted from nvim-treesitter (https://github.com/nvim-treesitter/nvim-treesitter)
; Licensed under the Apache License, Version 2.0
; Source: queries/just/locals.scm @ commit cf12346a3414fa1b06af75c79faebe7f76df080a
; Modifications: <see git history>

; This file tells us about the scope of variables so e.g. local
; variables override global functions with the same name

; Scope

(recipe) @local.scope

; Definitions

(alias
  left: (identifier) @local.definition)

(assignment
  left: (identifier) @local.definition)

(module
  name: (identifier) @local.definition)

(parameter
  name: (identifier) @local.definition)

(recipe_header
  name: (identifier) @local.definition)

; References

(alias
  right: (identifier) @local.reference)

(function_call
  name: (identifier) @local.reference)

(dependency
  name: (identifier) @local.reference)

(dependency_expression
  name: (identifier) @local.reference)

(value
  (identifier) @local.reference)
