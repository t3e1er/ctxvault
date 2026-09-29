;; Declarative Route Query Patterns (Axum & Actix)
(call_expression
  function: (field_expression field: (field_identifier) @_route_fn (#eq? @_route_fn "route"))
  arguments: (arguments
    (string_literal) @name
    (call_expression
      arguments: (arguments (identifier) @handles)))) @definition.route

(call_expression
  function: (field_expression field: (field_identifier) @_route_fn (#eq? @_route_fn "route"))
  arguments: (arguments
    (string_literal) @name
    (identifier) @handles)) @definition.route

(attribute_item
  (attribute
    (identifier) @_method (#match? @_method "^(get|post|put|delete|patch|head)$")
    (token_tree (string_literal) @name))) @definition.route