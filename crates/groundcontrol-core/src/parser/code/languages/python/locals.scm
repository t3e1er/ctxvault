;; Declarative Local Variable & Type Binding Patterns
(assignment
  left: (identifier) @local.var
  type: (type) @local.type)

(assignment
  left: (identifier) @local.var
  right: (call
    function: (identifier) @local.type))

(typed_parameter
  (identifier) @local.var
  type: (type) @local.type)

(typed_default_parameter
  name: (identifier) @local.var
  type: (type) @local.type)