(sub_definition
  name: (identifier) @name) @definition.method

(function_definition
  name: (identifier) @name) @definition.method

(property_definition
  name: (identifier) @name) @definition.method

(variable_definition
  name: (identifier) @name) @definition.variable

(call_statement
  name: (identifier) @name) @reference.call

(function_call
  name: (qualified_identifier) @name) @reference.call
