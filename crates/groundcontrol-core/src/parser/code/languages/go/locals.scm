;; Declarative Local Variable & Type Binding Patterns
(short_var_declaration
  left: (expression_list (identifier) @local.var)
  right: (expression_list (_) @local.type))

(parameter_declaration
  name: (identifier) @local.var
  type: (_) @local.type)