(declaration
  type: (_) @local.type
  declarator: (identifier) @local.var)

(declaration
  type: (_) @local.type
  declarator: (init_declarator
    declarator: (identifier) @local.var))

(parameter_declaration
  type: (_) @local.type
  declarator: (identifier) @local.var)