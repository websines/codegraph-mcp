;; TypeScript symbols query

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
  name: (type_identifier) @name
) @class

;; Method definitions
(method_definition
  name: (property_identifier) @name
  parameters: (formal_parameters) @params
) @method

;; Interface declarations
(interface_declaration
  name: (type_identifier) @name
) @interface

;; Type aliases
(type_alias_declaration
  name: (type_identifier) @name
) @type

;; Const/let/var declarations
(variable_declarator
  name: (identifier) @name
) @variable
