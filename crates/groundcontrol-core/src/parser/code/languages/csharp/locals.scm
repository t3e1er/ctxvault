(local_declaration_statement
  (variable_declaration
    type: (_) @local.type
    (variable_declarator
      name: (identifier) @local.var)))

(parameter
  type: (_) @local.type
  name: (identifier) @local.var)