;; JavaScript symbols query

;; Function declarations
(function_declaration
  name: (identifier) @name
  parameters: (formal_parameters) @params
) @function

;; Arrow functions (exported)
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (arrow_function) @arrow
  )
) @function

;; Class declarations
(class_declaration
  name: (identifier) @name
) @class

;; Method definitions
(method_definition
  name: (property_identifier) @name
  parameters: (formal_parameters) @params
) @method

;; Const/let/var declarations
(variable_declarator
  name: (identifier) @name
) @variable
