(function_definition
  (signature
    (call_expression
      (identifier) @name))) @definition.function

(struct_definition
  (type_head
    (identifier) @name)) @definition.struct

(abstract_definition
  (type_head
    (identifier) @name)) @definition.type

(module_definition
  name: (identifier) @name) @definition.module