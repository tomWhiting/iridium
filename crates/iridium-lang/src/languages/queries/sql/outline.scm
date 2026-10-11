(create_table
    (object_reference) @name) @item

; tree-sitter-sequel exposes the declared index name through its `column`
; field; the following object_reference is the indexed table.
(create_index
    column: (_) @name) @item

(create_function
    (object_reference) @name) @item

(create_view
    (object_reference) @name) @item
