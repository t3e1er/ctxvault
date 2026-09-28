(contract_declaration
  name: (identifier) @name
  (inheritance_specifier
    ancestor: (user_defined_type (identifier) @inherits))?) @definition.class

(interface_declaration
  name: (identifier) @name) @definition.interface

(library_declaration
  name: (identifier) @name) @definition.module

(function_definition
  name: (identifier) @name) @definition.function

(struct_declaration
  (identifier) @name) @definition.struct

(enum_declaration
  (identifier) @name) @definition.enum

(event_definition
  name: (identifier) @name) @definition.type

(error_declaration
  name: (identifier) @name) @definition.type