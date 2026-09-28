;; Rust tags and definition query pack

(function_item
  name: (identifier) @name) @definition.function

(function_signature_item
  name: (identifier) @name) @definition.function

(struct_item
  name: (type_identifier) @name) @definition.struct

(union_item
  name: (type_identifier) @name) @definition.struct

(enum_item
  name: (type_identifier) @name) @definition.enum

(trait_item
  name: (type_identifier) @name) @definition.trait

(mod_item
  name: (identifier) @name) @definition.module

(type_item
  name: (type_identifier) @name) @definition.type

(impl_item
  trait: (type_identifier)? @implements
  type: (_) @name) @definition.module

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


