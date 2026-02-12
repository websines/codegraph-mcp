;; JavaScript references query

;; Import statements
(import_statement
  source: (string) @source
) @import

;; Require calls
(call_expression
  function: (identifier) @require (#eq? @require "require")
  arguments: (arguments (string) @source)
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

;; Class extensions
(class_declaration
  (class_heritage
    value: (identifier) @superclass
  )
) @extends
