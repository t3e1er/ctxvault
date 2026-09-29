;; Declarative Local Variable & Type Binding Patterns
(variable_declarator
  name: (identifier) @local.var
  type: (type_annotation) @local.type)

(variable_declarator
  name: (identifier) @local.var
  value: (new_expression
    constructor: (identifier) @local.type))

(required_parameter
  pattern: (identifier) @local.var
  type: (type_annotation) @local.type)

(optional_parameter
  pattern: (identifier) @local.var
  type: (type_annotation) @local.type)