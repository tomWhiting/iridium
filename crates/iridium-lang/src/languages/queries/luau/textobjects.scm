; From https://github.com/4teapo/zed-luau/blob/8074b0b860301957f131957f0c09fc49071adf13/languages/luau/textobjects.scm (MIT)
(function_definition
  body: (_)* @function.inside) @function.around

(function_declaration
  body: (_)* @function.inside) @function.around

(local_function_declaration
  body: (_)* @function.inside) @function.around

(const_function_declaration
  body: (_)* @function.inside) @function.around

(type_alias_declaration
  type: (_)* @class.inside) @class.around

(type_function_declaration
  body: (_)* @function.inside) @function.around

(comment)+ @comment.around
