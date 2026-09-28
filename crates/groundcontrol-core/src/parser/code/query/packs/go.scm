;; Go tags and definition query pack

(function_declaration
  name: (identifier) @name) @definition.function

(method_declaration
  name: (field_identifier) @name) @definition.method

(type_spec
  name: (type_identifier) @name
  type: (struct_type)) @definition.struct

(type_spec
  name: (type_identifier) @name
  type: (struct_type
    (field_declaration_list
      (field_declaration
        type: (type_identifier) @struct_embeds
        !name)))) @definition.struct

(type_spec
  name: (type_identifier) @name
  type: (interface_type)) @definition.interface

(type_spec
  name: (type_identifier) @name) @definition.type

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

