;; TypeScript references query

;; Import statements
(import_statement
  source: (string) @source
) @import

;; Function calls
(call_expression
  function: (identifier) @name
) @call

;; Method calls
(call_expression
  function: (member_expression
    property: (property_identifier) @name
  )
) @call

;; Class extensions (extends in class heritage)
(class_declaration
  (class_heritage
    (extends_clause
      value: (identifier) @superclass
    )
  )
) @extends
