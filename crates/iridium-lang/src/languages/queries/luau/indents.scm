; From https://github.com/4teapo/zed-luau/blob/8074b0b860301957f131957f0c09fc49071adf13/languages/luau/indents.scm (MIT)
(if_statement "end" @end) @indent
(do_statement "end" @end) @indent
(while_statement "end" @end) @indent
(for_statement "end" @end) @indent
(repeat_statement "until" @end) @indent
(function_declaration "end" @end) @indent
(local_function_declaration "end" @end) @indent
(const_function_declaration "end" @end) @indent
(type_function_declaration "end" @end) @indent
(function_definition "end" @end) @indent

(_ "[" "]" @end) @indent
(_ "{" "}" @end) @indent
(_ "(" ")" @end) @indent
