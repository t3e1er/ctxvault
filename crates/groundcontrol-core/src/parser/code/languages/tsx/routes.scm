;; Declarative Route Query Patterns (Express / Fastify / Koa)
(call_expression
  function: (member_expression
    property: (property_identifier) @_method (#match? @_method "^(get|post|put|delete|patch|all|options|head)$"))
  arguments: (arguments
    (string) @name
    [(identifier) (arrow_function) (function_expression)] @handles)) @definition.route

(call_expression
  function: (member_expression
    property: (property_identifier) @_method (#match? @_method "^(get|post|put|delete|patch|all|options|head)$"))
  arguments: (arguments
    (string) @name)) @definition.route