;; Declarative Local Variable & Type Binding Patterns
(let_declaration
  pattern: (identifier) @local.var
  type: (_) @local.type)

(let_declaration
  pattern: (identifier) @local.var
  value: (call_expression
    function: (scoped_identifier
      path: (identifier) @local.type)))

(let_declaration
  pattern: (identifier) @local.var
  value: (struct_expression
    name: (type_identifier) @local.type))

(parameter
  pattern: (identifier) @local.var
  type: (_) @local.type)