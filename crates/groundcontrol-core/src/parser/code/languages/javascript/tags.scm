(function_declaration
  name: (identifier) @name) @definition.function

(method_definition
  name: (property_identifier) @name) @definition.method

(class_declaration
  name: (type_identifier) @name) @definition.class

(class_declaration
  name: (type_identifier) @name
  (class_heritage
    (extends_clause
      value: (_) @inherits))) @definition.class

(class_declaration
  name: (type_identifier) @name
  (class_heritage
    (implements_clause
      (type_identifier) @implements))) @definition.class

(interface_declaration
  name: (type_identifier) @name) @definition.interface

(interface_declaration
  name: (type_identifier) @name
  (extends_type_clause
    (type_identifier) @inherits)) @definition.interface

(type_alias_declaration
  name: (type_identifier) @name) @definition.type

(enum_declaration
  name: (identifier) @name) @definition.enum

(module
  name: (identifier) @name) @definition.module

(internal_module
  name: (identifier) @name) @definition.module