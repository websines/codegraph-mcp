;; Go references query

;; Import declarations
(import_declaration
  (import_spec
    path: (interpreted_string_literal) @path
  )
) @import

;; Function calls
(call_expression
  function: (identifier) @name
) @call

;; Method calls
(call_expression
  function: (selector_expression
    field: (field_identifier) @name
  )
) @call
