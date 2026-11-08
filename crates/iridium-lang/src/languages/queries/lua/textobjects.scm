; From https://github.com/zed-extensions/lua/blob/ce3fde1f675b32ce2de032e32609aba18cbf8121/languages/lua/textobjects.scm (Apache-2.0)
(function_definition
  body: (_) @function.inside) @function.around

(function_declaration
  body: (_) @function.inside) @function.around

(comment)+ @comment.around
