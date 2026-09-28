(function_definition
  name: (identifier) @name) @definition.function

(function_declaration
  name: (identifier) @name) @definition.function

(class_definition
  name: (identifier) @name
  extend: (extends_clause
    type: (type_identifier) @inherits)?) @definition.class

(object_definition
  name: (identifier) @name
  extend: (extends_clause
    type: (type_identifier) @inherits)?) @definition.class

(trait_definition
  name: (identifier) @name
  extend: (extends_clause
    type: (type_identifier) @inherits)?) @definition.trait