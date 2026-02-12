;; Rust references query - extracts function calls, imports, etc.

;; Use declarations (imports)
(use_declaration
  argument: (_) @import
) @use

;; Function calls
(call_expression
  function: (identifier) @name
) @call

;; Method calls
(call_expression
  function: (field_expression
    field: (field_identifier) @name
  )
) @call

;; Trait implementations
(impl_item
  trait: (type_identifier) @trait
  type: (type_identifier) @type
) @implements
