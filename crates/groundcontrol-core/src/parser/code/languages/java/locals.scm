(local_variable_declaration
  type: (_) @local.type
  declarator: (variable_declarator
    name: (identifier) @local.var))

(variable_declarator
  name: (identifier) @local.var
  value: (object_creation_expression
    type: (_) @local.type))

(formal_parameter
  type: (_) @local.type
  name: (identifier) @local.var)