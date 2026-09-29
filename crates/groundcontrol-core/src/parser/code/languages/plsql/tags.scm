(create_package
  package_name: (identifier) @name) @definition.module

(create_package_body
  package_name: (identifier) @name) @definition.module

(procedure_definition
  prc_name: (identifier) @name) @definition.function

(procedure_declaration
  prc_name: (identifier) @name) @definition.function

(function_definition
  fnc_name: (identifier) @name) @definition.function

(function_declaration
  fnc_name: (identifier) @name) @definition.function

(create_type
  type_name: (identifier) @name) @definition.struct

(create_type_body
  type_name: (identifier) @name) @definition.struct

(create_trigger
  trigger_name: (identifier) @name) @definition.function
