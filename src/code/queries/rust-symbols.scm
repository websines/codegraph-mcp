;; Rust symbols query - extracts functions, structs, enums, traits, etc.

;; Functions
(function_item
  name: (identifier) @name
  parameters: (parameters) @params
) @function

;; Struct definitions
(struct_item
  name: (type_identifier) @name
) @struct

;; Enum definitions
(enum_item
  name: (type_identifier) @name
) @enum

;; Trait definitions
(trait_item
  name: (type_identifier) @name
) @trait

;; Impl blocks
(impl_item
  trait: (type_identifier)? @trait_name
  type: (type_identifier) @type_name
) @impl

;; Type aliases
(type_item
  name: (type_identifier) @name
) @type

;; Constants
(const_item
  name: (identifier) @name
) @const

;; Static items
(static_item
  name: (identifier) @name
) @static

;; Modules
(mod_item
  name: (identifier) @name
) @module
