(function_definition
  name: (identifier) @name) @definition.function

(class_definition
  name: (identifier) @name
  superclasses: (argument_list
    (identifier) @inherits)?) @definition.class