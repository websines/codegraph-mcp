;; Python references query

;; Import statements
(import_statement
  name: (dotted_name) @module
) @import

;; From imports
(import_from_statement
  module_name: (dotted_name) @module
) @import

;; Function calls
(call
  function: (identifier) @name
) @call

;; Method calls
(call
  function: (attribute
    attribute: (identifier) @name
  )
) @call

;; Class inheritance
(class_definition
  superclasses: (argument_list
    (identifier) @superclass
  )
) @extends
