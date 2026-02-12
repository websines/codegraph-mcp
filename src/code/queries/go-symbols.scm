;; Go symbols query

;; Function declarations
(function_declaration
  name: (identifier) @name
  parameters: (parameter_list) @params
) @function

;; Method declarations
(method_declaration
  name: (field_identifier) @name
  parameters: (parameter_list) @params
  receiver: (parameter_list) @receiver
) @method

;; Type declarations (structs, interfaces)
(type_declaration
  (type_spec
    name: (type_identifier) @name
    type: (struct_type)
  )
) @struct

(type_declaration
  (type_spec
    name: (type_identifier) @name
    type: (interface_type)
  )
) @interface

;; Const declarations
(const_declaration
  (const_spec
    name: (identifier) @name
  )
) @const

;; Var declarations
(var_declaration
  (var_spec
    name: (identifier) @name
  )
) @variable
