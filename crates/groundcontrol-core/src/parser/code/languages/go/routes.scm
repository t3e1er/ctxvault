;; Declarative Route Query Patterns (Gin, Echo, Chi, net/http)
(call_expression
  function: (selector_expression
    field: (field_identifier) @_method (#match? @_method "^(GET|POST|PUT|DELETE|PATCH|Any|Handle|HandleFunc)$"))
  arguments: (argument_list
    [(interpreted_string_literal) (raw_string_literal)] @name
    (identifier) @handles)) @definition.route

(call_expression
  function: (selector_expression
    field: (field_identifier) @_method (#match? @_method "^(GET|POST|PUT|DELETE|PATCH|Any|Handle|HandleFunc)$"))
  arguments: (argument_list
    [(interpreted_string_literal) (raw_string_literal)] @name)) @definition.route