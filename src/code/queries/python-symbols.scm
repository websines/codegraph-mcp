;; Python symbols query

;; Function definitions
(function_definition
  name: (identifier) @name
  parameters: (parameters) @params
) @function

;; Class definitions
(class_definition
  name: (identifier) @name
) @class

;; Decorated definitions (methods with decorators)
(decorated_definition
  definition: (function_definition
    name: (identifier) @name
  )
) @function

;; Global assignments (module-level constants)
(expression_statement
  (assignment
    left: (identifier) @name
  )
) @variable
